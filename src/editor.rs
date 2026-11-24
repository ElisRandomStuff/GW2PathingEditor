//! Editor state and actions, independent of Nexus and ImGui.

use crate::{
    archive,
    attrs::Attrs,
    clipboard, files,
    format::fmt_float,
    geometry::{add, distance, sub},
    guid, namespace,
    pack::{self, Category, LoadReport, Pack, Poi},
    settings::Settings,
    trail::{self, Trail},
};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

/// File the pack persists in, inside the addon directory.
pub const CAPTURES_FILE: &str = "captures.xml";
pub const SETTINGS_FILE: &str = "settings.ini";

/// Categories put in the input boxes when there's nothing to restore them from.
pub const DEFAULT_CATEGORY: &str = "mypack.markers";
pub const DEFAULT_TRAIL_CATEGORY: &str = "mypack.trails";

/// Radius of the duplicate guard. Same as the default `triggerRange` (TacO uses the marker's
/// trigger range for its own guard, docs/06 §1.1).
pub const DUPLICATE_RANGE_M: f32 = 2.0;

/// Moving farther than this from one frame to the next is a teleport (waypoint, portal,
/// respawn), not movement. The recorder pauses instead of drawing a line across the map.
pub const TELEPORT_M: f32 = 50.0;

/// Resuming within this distance of the trail's last point continues its line. Farther away,
/// the trail gets a new section, so no line is drawn across the gap.
pub const RESUME_JOIN_M: f32 = 5.0;

/// Points recorded between two writes of the `.trl` while recording.
const SAVE_EVERY_POINTS: usize = 20;

/// Most undo steps kept.
const UNDO_LIMIT: usize = 100;

const NOT_ON_MAP: &str = "Not on a map (no MumbleLink data yet).";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Warning,
    Error,
}

/// Last message for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub kind: StatusKind,
    pub text: String,
}

/// How recording more of a trail connects to what's already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// Continue the line within [`RESUME_JOIN_M`] of the trail's last point, else start a new
    /// section.
    Auto,
    /// Continue the line of this section (0-based) from its last point, however far it is.
    /// The section is moved after the others (their order changes nothing on screen).
    Section(usize),
    /// Start a new section.
    Never,
}

/// What the pack window's details pane shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// A category namespace.
    Category(String),
    /// Indices in `pack.pois`, never empty.
    Pois(BTreeSet<usize>),
    /// An index in `pack.trails`.
    Trail(usize),
}

impl Selection {
    /// One marker.
    pub fn poi(index: usize) -> Self {
        Self::Pois(BTreeSet::from([index]))
    }
}

pub struct Editor {
    pub pack: Pack,
    /// Raw text of the marker category box. Sanitised when a marker is captured.
    pub category_input: String,
    /// Raw text of the trail category box. Sanitised when a trail is started.
    pub trail_category_input: String,
    pub settings: Settings,
    pub status: Option<Status>,
    pub selection: Option<Selection>,
    /// Points of the selected trail (indices in its `points`), for the pack window and the
    /// preview.
    pub selected_points: BTreeSet<usize>,
    recorder: Option<Recorder>,
    undo: Vec<UndoStep>,
    redo: Vec<UndoStep>,
    /// The addon directory. `trailData`, `iconFile` and `texture` paths are relative to it.
    dir: PathBuf,
    /// `settings` as last written to disk.
    saved_settings: Settings,
    /// False when the captures file exists but couldn't be read: saving would overwrite it.
    persist: bool,
}

/// What an undo step restores.
enum Snapshot {
    /// Every marker.
    Pois(Vec<Poi>),
    /// The points of the trail with this GUID.
    Trail(String, Vec<[f32; 3]>),
    /// The category tree, every marker, and each trail's category and attributes (by GUID,
    /// not its points): for changes to the pack's structure, like renaming a category.
    Structure {
        categories: Vec<Category>,
        pois: Vec<Poi>,
        trails: Vec<(String, String, Attrs)>,
    },
}

struct UndoStep {
    label: String,
    snapshot: Snapshot,
}

/// The trail being recorded.
struct Recorder {
    /// GUID of the trail in `pack.trails` (indices shift when other trails are deleted).
    guid: String,
    paused: bool,
    /// Position on the last in-game frame, to notice teleports.
    last_seen: Option<[f32; 3]>,
    /// Points added since the `.trl` was last written.
    unsaved: usize,
}

impl Editor {
    /// Opens the editor on `dir` (the addon directory): loads the captures file, the trails'
    /// `.trl` files and the settings.
    pub fn open(dir: &Path) -> Self {
        let file = dir.join(CAPTURES_FILE);
        let mut problems = Vec::new();
        let (report, persist) = match pack::load_file(&file) {
            Ok(report) => (report, true),
            Err(text) => {
                problems.push(format!(
                    "{text}. Changes won't be saved until the addon is reloaded."
                ));
                (LoadReport::default(), false)
            }
        };
        let LoadReport { mut pack, warning } = report;
        problems.extend(warning);
        pack.ensure_used_categories();

        let failed: Vec<String> = pack
            .trails
            .iter_mut()
            .filter_map(|t| trail::load(dir, t).err())
            .collect();
        if let Some(first) = failed.first() {
            problems.push(format!(
                "{} trail file(s) couldn't be read ({first}). Those trails are kept, without points.",
                failed.len()
            ));
        }

        let status = (!problems.is_empty()).then(|| Status {
            kind: if persist {
                StatusKind::Warning
            } else {
                StatusKind::Error
            },
            text: problems.join(" "),
        });
        let settings = Settings::load(&dir.join(SETTINGS_FILE));

        Self {
            category_input: last_category(pack.pois.iter().map(|p| &p.category), DEFAULT_CATEGORY),
            trail_category_input: last_category(
                pack.trails.iter().map(|t| &t.category),
                DEFAULT_TRAIL_CATEGORY,
            ),
            pack,
            saved_settings: settings.clone(),
            settings,
            status,
            selection: None,
            selected_points: BTreeSet::new(),
            recorder: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dir: dir.to_path_buf(),
            persist,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(CAPTURES_FILE)
    }

    /// The namespace new markers get, i.e. the sanitised category input.
    pub fn category(&self) -> String {
        namespace::sanitize(&self.category_input)
    }

    /// The namespace new trails get.
    pub fn trail_category(&self) -> String {
        namespace::sanitize(&self.trail_category_input)
    }

    /// Where a marker placed with the character standing at `feet` goes: raised by the "raise
    /// markers" setting.
    pub fn marker_pos(&self, feet: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = feet;
        [x, y + self.settings.marker_raise(), z]
    }

    /// Shows `text` as a warning and returns it as the error.
    fn refuse<T>(&mut self, text: impl Into<String>) -> Result<T, String> {
        let text = text.into();
        self.set_status(StatusKind::Warning, text.clone());
        Err(text)
    }

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status = Some(Status {
            kind,
            text: text.into(),
        });
    }

    /// Saves after the UI changed the pack in place (a dragged position, an attribute), and says
    /// so in the status.
    pub fn commit(&mut self, text: impl Into<String>) {
        self.set_status(StatusKind::Info, text);
        self.save();
    }

    // ---- Undo ----

    /// Remembers every marker, so the next change to them can be undone. Changes made through
    /// the editor's methods do it themselves; call it before changing markers in place.
    pub fn checkpoint_pois(&mut self, label: impl Into<String>) {
        let snapshot = Snapshot::Pois(self.pack.pois.clone());
        self.push_undo(label.into(), snapshot);
    }

    /// Remembers the points of trail `index`, so the next change to them can be undone.
    pub fn checkpoint_trail(&mut self, index: usize, label: impl Into<String>) {
        if let Some(snapshot) = self.trail_snapshot(index) {
            self.push_undo(label.into(), snapshot);
        }
    }

    fn structure_snapshot(&self) -> Snapshot {
        Snapshot::Structure {
            categories: self.pack.categories.clone(),
            pois: self.pack.pois.clone(),
            trails: self
                .pack
                .trails
                .iter()
                .map(|t| (t.guid.clone(), t.category.clone(), t.attrs.clone()))
                .collect(),
        }
    }

    fn trail_snapshot(&self, index: usize) -> Option<Snapshot> {
        let trail = self.pack.trails.get(index)?;
        Some(Snapshot::Trail(trail.guid.clone(), trail.points.clone()))
    }

    fn push_undo(&mut self, label: String, snapshot: Snapshot) {
        self.redo.clear();
        self.undo.push(UndoStep { label, snapshot });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    /// What [`Editor::undo`] would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    pub fn undo(&mut self) {
        self.step_history(true);
    }

    pub fn redo(&mut self) {
        self.step_history(false);
    }

    fn step_history(&mut self, undo: bool) {
        let step = if undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        };
        let Some(step) = step else {
            let text = if undo {
                "Nothing to undo."
            } else {
                "Nothing to redo."
            };
            self.set_status(StatusKind::Warning, text);
            return;
        };
        // The state it replaces, for the opposite direction.
        let current = match &step.snapshot {
            Snapshot::Pois(_) => Some(Snapshot::Pois(self.pack.pois.clone())),
            Snapshot::Structure { .. } => Some(self.structure_snapshot()),
            Snapshot::Trail(guid, _) => {
                let index = self.pack.trails.iter().position(|t| t.guid == *guid);
                if index.is_some() && index == self.recording().map(|(i, _)| i) {
                    let label = step.label.clone();
                    if undo {
                        self.undo.push(step);
                    } else {
                        self.redo.push(step);
                    }
                    self.set_status(
                        StatusKind::Warning,
                        format!("Stop recording that trail first ({label})."),
                    );
                    return;
                }
                index.and_then(|i| self.trail_snapshot(i))
            }
        };
        let Some(current) = current else {
            self.set_status(
                StatusKind::Warning,
                format!(
                    "Can't {} \"{}\": that trail was deleted.",
                    if undo { "undo" } else { "redo" },
                    step.label
                ),
            );
            return;
        };

        let UndoStep { label, snapshot } = step;
        match snapshot {
            Snapshot::Pois(pois) => {
                self.pack.pois = pois;
                self.pack.ensure_used_categories();
                let count = self.pack.pois.len();
                if let Some(Selection::Pois(set)) = &mut self.selection {
                    set.retain(|&i| i < count);
                    if set.is_empty() {
                        self.selection = None;
                    }
                }
                self.save();
            }
            Snapshot::Structure {
                categories,
                pois,
                trails,
            } => {
                self.pack.categories = categories;
                self.pack.pois = pois;
                for (guid, category, attrs) in trails {
                    if let Some(trail) = self.pack.trails.iter_mut().find(|t| t.guid == guid) {
                        trail.category = category;
                        trail.attrs = attrs;
                    }
                }
                self.pack.ensure_used_categories();
                let count = self.pack.pois.len();
                match &mut self.selection {
                    Some(Selection::Pois(set)) => {
                        set.retain(|&i| i < count);
                        if set.is_empty() {
                            self.selection = None;
                        }
                    }
                    Some(Selection::Category(ns)) if self.pack.category(ns).is_none() => {
                        self.selection = None;
                    }
                    _ => {}
                }
                self.save();
            }
            Snapshot::Trail(guid, points) => {
                if let Some(index) = self.pack.trails.iter().position(|t| t.guid == guid) {
                    self.pack.trails[index].points = points;
                    if self.selection == Some(Selection::Trail(index)) {
                        self.selected_points.clear();
                    }
                    self.save_trail(index);
                }
            }
        }
        let text = format!("{}: {label}", if undo { "Undid" } else { "Redid" });
        let step = UndoStep {
            label,
            snapshot: current,
        };
        if undo {
            self.redo.push(step);
        } else {
            self.undo.push(step);
        }
        self.set_status(StatusKind::Info, text);
    }

    // ---- Markers ----

    /// Captures a marker for the character standing at `feet` on `map_id`, in the category of
    /// the marker category box. Returns the new marker's index.
    pub fn capture(&mut self, map_id: u32, feet: [f32; 3]) -> Result<usize, String> {
        let category = self.category();
        if category.is_empty() {
            return self.refuse("Set a category first.");
        }
        self.add_poi(&category, map_id, feet)
    }

    /// Adds a marker in `category` for the character standing at `feet` on `map_id`. Returns
    /// its index.
    pub fn add_poi(
        &mut self,
        category: &str,
        map_id: u32,
        feet: [f32; 3],
    ) -> Result<usize, String> {
        let result = self.try_add_poi(category, map_id, feet);
        match &result {
            Ok(index) => {
                let text = format!("Added marker #{} ({category})", index + 1);
                self.commit(text);
            }
            Err(text) => self.set_status(StatusKind::Warning, text.clone()),
        }
        result
    }

    fn try_add_poi(
        &mut self,
        category: &str,
        map_id: u32,
        feet: [f32; 3],
    ) -> Result<usize, String> {
        if map_id == 0 {
            return Err(NOT_ON_MAP.into());
        }
        let pos = self.marker_pos(feet);
        let near = |p: &Poi| {
            p.map_id == map_id
                && p.category.eq_ignore_ascii_case(category)
                && p.distance_to(pos) < DUPLICATE_RANGE_M
        };
        if self.settings.skip_near_duplicates
            && let Some(i) = self.pack.pois.iter().position(near)
        {
            return Err(format!(
                "Skipped: marker #{} is also {category} and within {DUPLICATE_RANGE_M} m.",
                i + 1
            ));
        }
        self.checkpoint_pois("add a marker");
        self.pack.ensure_category(category);
        self.pack.pois.push(Poi::new(map_id, pos, category));
        Ok(self.pack.pois.len() - 1)
    }

    /// Moves a marker to where the character stands (raised like new markers), on `map_id`.
    pub fn move_poi(&mut self, index: usize, map_id: u32, feet: [f32; 3]) -> Result<(), String> {
        if map_id == 0 {
            return self.refuse(NOT_ON_MAP);
        }
        let pos = self.marker_pos(feet);
        if index >= self.pack.pois.len() {
            return Err("No such marker.".into());
        }
        self.checkpoint_pois(format!("move marker #{}", index + 1));
        let poi = &mut self.pack.pois[index];
        let from = (poi.map_id != map_id).then_some(poi.map_id);
        poi.map_id = map_id;
        poi.pos = pos;
        let text = match from {
            Some(from) => format!(
                "Moved marker #{} from map {from} to map {map_id}.",
                index + 1
            ),
            None => format!("Moved marker #{} here.", index + 1),
        };
        self.commit(text);
        Ok(())
    }

    /// Moves a marker to another category, from user input (sanitised).
    pub fn set_poi_category(&mut self, index: usize, input: &str) -> Result<(), String> {
        self.set_pois_category(&BTreeSet::from([index]), input)
    }

    /// Moves markers to another category, from user input (sanitised).
    pub fn set_pois_category(
        &mut self,
        indices: &BTreeSet<usize>,
        input: &str,
    ) -> Result<(), String> {
        let category = self.checked_category(input)?;
        let indices = self.valid_pois(indices);
        if indices.is_empty() {
            return Err("No such marker.".into());
        }
        self.checkpoint_pois(format!("move {} to {category}", markers_label(&indices)));
        for &i in &indices {
            self.pack.pois[i].category = category.clone();
        }
        self.pack.ensure_category(&category);
        self.commit(format!("{} now in {category}.", markers_label(&indices)));
        Ok(())
    }

    pub fn remove_poi(&mut self, index: usize) {
        self.remove_pois(&BTreeSet::from([index]));
    }

    /// Deletes markers.
    pub fn remove_pois(&mut self, indices: &BTreeSet<usize>) {
        let removed = self.valid_pois(indices);
        if removed.is_empty() {
            return;
        }
        let label = markers_label(&removed);
        self.checkpoint_pois(format!("delete {label}"));
        for &i in removed.iter().rev() {
            self.pack.pois.remove(i);
        }
        if let Some(Selection::Pois(set)) = &mut self.selection {
            *set = set
                .iter()
                .filter(|i| !removed.contains(i))
                .map(|&i| i - removed.iter().filter(|&&r| r < i).count())
                .collect();
            if set.is_empty() {
                self.selection = None;
            }
        }
        self.commit(format!("Deleted {label}."));
    }

    /// Deletes every marker (trails and categories stay).
    pub fn clear_pois(&mut self) {
        let n = self.pack.pois.len();
        self.checkpoint_pois(format!("delete all {n} markers"));
        self.pack.pois.clear();
        if matches!(self.selection, Some(Selection::Pois(_))) {
            self.selection = None;
        }
        self.commit(format!("Deleted {n} marker(s)"));
    }

    /// The markers of the selection, if it's markers.
    pub fn selected_pois(&self) -> Option<&BTreeSet<usize>> {
        match &self.selection {
            Some(Selection::Pois(set)) => Some(set),
            _ => None,
        }
    }

    /// Moves markers by `delta` metres, in place: call [`Editor::checkpoint_pois`] before the
    /// first move and [`Editor::commit`] after the last.
    pub fn offset_pois(&mut self, indices: &BTreeSet<usize>, delta: [f32; 3]) {
        for i in self.valid_pois(indices) {
            let poi = &mut self.pack.pois[i];
            poi.pos = add(poi.pos, delta);
        }
    }

    /// Moves markers together so that the first one is where the character stands (raised like
    /// new markers), on `map_id`, keeping how they're laid out.
    pub fn move_pois_here(
        &mut self,
        indices: &BTreeSet<usize>,
        map_id: u32,
        feet: [f32; 3],
    ) -> Result<(), String> {
        let indices = self.valid_pois(indices);
        let result = match indices.first() {
            _ if map_id == 0 => Err(NOT_ON_MAP.to_owned()),
            None => Err("No such marker.".to_owned()),
            Some(&first) => Ok(first),
        };
        let first = match result {
            Ok(first) => first,
            Err(text) => return self.refuse(text),
        };
        let label = markers_label(&indices);
        self.checkpoint_pois(format!("move {label}"));
        let delta = sub(self.marker_pos(feet), self.pack.pois[first].pos);
        for &i in &indices {
            let poi = &mut self.pack.pois[i];
            poi.pos = add(poi.pos, delta);
            poi.map_id = map_id;
        }
        self.commit(format!("Moved {label} here."));
        Ok(())
    }

    /// Adds copies of `pois` (with new GUIDs), in `category` if given. With `here`, they move
    /// together so that the first one is where the character stands, on that map. Selects and
    /// returns them.
    pub fn paste_pois(
        &mut self,
        pois: Vec<Poi>,
        here: Option<(u32, [f32; 3])>,
        category: Option<&str>,
    ) -> Result<Vec<usize>, String> {
        if pois.is_empty() {
            return Err("Nothing to paste.".into());
        }
        if here.is_some_and(|(map_id, _)| map_id == 0) {
            return self.refuse(NOT_ON_MAP);
        }
        let shift = here.map(|(map_id, feet)| (map_id, sub(self.marker_pos(feet), pois[0].pos)));
        self.checkpoint_pois(format!("paste {} marker(s)", pois.len()));
        let mut added = Vec::new();
        for mut poi in pois {
            poi.guid = guid::new_guid();
            if let Some((map_id, delta)) = shift {
                poi.pos = add(poi.pos, delta);
                poi.map_id = map_id;
            }
            if let Some(category) = category {
                poi.category = category.to_owned();
            }
            self.pack.ensure_category(&poi.category);
            self.pack.pois.push(poi);
            added.push(self.pack.pois.len() - 1);
        }
        self.selection = Some(Selection::Pois(added.iter().copied().collect()));
        self.commit(format!("Pasted {} marker(s).", added.len()));
        Ok(added)
    }

    fn valid_pois(&self, indices: &BTreeSet<usize>) -> Vec<usize> {
        let count = self.pack.pois.len();
        indices.iter().copied().filter(|&i| i < count).collect()
    }

    /// All markers as `<POI>` lines.
    pub fn all_poi_xml(&self) -> String {
        clipboard::pois_text(&self.pack.pois)
    }

    // ---- Categories ----

    /// Adds a category under `parent` (empty: at the top), from user input (sanitised).
    /// Returns its namespace.
    pub fn add_category(&mut self, parent: &str, input: &str) -> Result<String, String> {
        let result = namespace::sanitize(input);
        let result = if result.is_empty() {
            Err("Enter a name (a-z, 0-9 and _).".to_owned())
        } else {
            let ns = namespace::join(parent, &result);
            if self.pack.category(&ns).is_some() {
                Err(format!("{ns} already exists."))
            } else {
                Ok(ns)
            }
        };
        match &result {
            Ok(ns) => {
                self.pack.ensure_category(ns);
                self.commit(format!("Added category {ns}."));
            }
            Err(text) => self.set_status(StatusKind::Warning, text.clone()),
        }
        result
    }

    /// Deletes a category and its subcategories, if no marker or trail is in them.
    pub fn remove_category(&mut self, ns: &str) -> Result<(), String> {
        let result = self.pack.remove_category(ns);
        match &result {
            Ok(()) => {
                if let Some(Selection::Category(selected)) = &self.selection
                    && namespace::is_within(selected, ns)
                {
                    self.selection = None;
                }
                self.commit(format!("Deleted category {ns}."));
            }
            Err(text) => self.set_status(StatusKind::Warning, text.clone()),
        }
        result
    }

    /// Renames or moves category `ns` to `input` (sanitized), with everything in it: see
    /// [`Pack::rename_category`]. Merges into it if it exists. Undoable. Returns the new
    /// namespace.
    pub fn rename_category(&mut self, ns: &str, input: &str) -> Result<String, String> {
        let new = self.checked_category(input)?;
        let before = self.structure_snapshot();
        let renamed = match self.pack.rename_category(ns, &new) {
            Ok(renamed) => renamed,
            Err(text) => return self.refuse(text),
        };
        self.push_undo(format!("rename {ns} to {new}"), before);

        let rebase = |value: &str| -> Option<String> {
            namespace::is_within(value, ns).then(|| {
                let rest: Vec<&str> = namespace::segments(value)
                    .skip(namespace::segments(ns).count())
                    .collect();
                std::iter::once(new.as_str())
                    .chain(rest)
                    .collect::<Vec<_>>()
                    .join(".")
            })
        };
        if let Some(Selection::Category(selected)) = &self.selection
            && let Some(moved) = rebase(selected)
        {
            self.selection = Some(Selection::Category(moved));
        }
        for input in [&mut self.category_input, &mut self.trail_category_input] {
            if let Some(moved) = rebase(&namespace::sanitize(input)) {
                *input = moved;
            }
        }
        let hidden = std::mem::take(&mut self.settings.hidden_categories);
        self.settings.hidden_categories = hidden
            .into_iter()
            .map(|h| rebase(&h).map(|m| m.to_lowercase()).unwrap_or(h))
            .collect();

        let mut text = format!(
            "{} {ns} to {new}: {} marker(s), {} trail(s).",
            if renamed.merged { "Merged" } else { "Renamed" },
            renamed.pois,
            renamed.trails
        );
        if renamed.references > 0 {
            text.push_str(&format!(
                " {} toggleCategory value(s) updated.",
                renamed.references
            ));
        }
        if renamed.kept > 0 {
            text.push_str(&format!(
                " {} attribute(s) both set: {new}'s values were kept.",
                renamed.kept
            ));
        }
        let scripts = self.scripts_naming(ns);
        let kind = if scripts.is_empty() {
            StatusKind::Info
        } else {
            text.push_str(&format!(
                " {} mention {ns}: scripts aren't changed, update them by hand.",
                short_list(scripts.iter().map(String::as_str))
            ));
            StatusKind::Warning
        };
        self.set_status(kind, text);
        self.save();
        Ok(new)
    }

    /// The `.lua` files whose text contains `ns` (any casing), e.g. in `World:CategoryByType`.
    fn scripts_naming(&self, ns: &str) -> Vec<String> {
        let needle = ns.to_lowercase();
        crate::scripts::LuaScan::scan(&self.dir)
            .files
            .into_iter()
            .filter(|f| {
                files::resolve(&self.dir, &f.path)
                    .and_then(|p| fs::read_to_string(p).ok())
                    .is_some_and(|text| text.to_lowercase().contains(&needle))
            })
            .map(|f| f.path)
            .collect()
    }

    fn checked_category(&mut self, input: &str) -> Result<String, String> {
        let category = namespace::sanitize(input);
        if category.is_empty() {
            return self.refuse("Enter a category namespace, e.g. mypack.chests");
        }
        Ok(category)
    }

    // ---- Trails ----

    /// All trails as `<Trail>` lines.
    pub fn all_trail_xml(&self) -> String {
        clipboard::trails_text(&self.pack.trails)
    }

    /// Moves a trail to another category, from user input (sanitised).
    pub fn set_trail_category(&mut self, index: usize, input: &str) -> Result<(), String> {
        let category = self.checked_category(input)?;
        let Some(trail) = self.pack.trails.get_mut(index) else {
            return Err("No such trail.".into());
        };
        trail.category = category.clone();
        self.pack.ensure_category(&category);
        self.commit(format!("Trail #{} is now in {category}.", index + 1));
        Ok(())
    }

    /// Adds copies of `trails` (`<Trail>` lines pasted from a pack). Each one's points are read
    /// from the `.trl` its `trailData` names, looked for at that path in the addon directory,
    /// then by file name in `trails/` and in the addon directory. The copy gets its own `.trl`
    /// and a new GUID, so editing or deleting it never touches the file it came from. In
    /// `category` if given; with `here`, moved so its first point is at that position, on that
    /// map. Returns the new trails' indices; the status names the files not found.
    pub fn paste_trails(
        &mut self,
        trails: Vec<Trail>,
        here: Option<(u32, [f32; 3])>,
        category: Option<&str>,
    ) -> Result<Vec<usize>, String> {
        if here.is_some_and(|(map_id, _)| map_id == 0) {
            return self.refuse(NOT_ON_MAP);
        }
        let mut added = Vec::new();
        let mut missing = Vec::new();
        for mut new in trails {
            let Some(found) = self.find_trl(&new.file) else {
                missing.push(new.file.clone());
                continue;
            };
            let mut source = new.clone();
            source.file = found;
            if let Err(e) = trail::load(&self.dir, &mut source) {
                missing.push(e);
                continue;
            }
            new.map_id = source.map_id;
            new.points = source.points;
            if let Some((map_id, pos)) = here
                && let Some(first) = new.points.iter().copied().find(|&p| !trail::is_break(p))
            {
                let delta = sub(pos, first);
                for p in new.points.iter_mut().filter(|p| !trail::is_break(**p)) {
                    *p = add(*p, delta);
                }
                new.map_id = map_id;
            }
            new.guid = guid::new_guid();
            if let Some(category) = category {
                new.category = category.to_owned();
            }
            self.pack.ensure_category(&new.category);
            new.file = trail::new_file_name(&self.dir, new.map_id, &self.pack.trails);
            self.pack.trails.push(new);
            added.push(self.pack.trails.len() - 1);
            self.save_trail(self.pack.trails.len() - 1);
        }

        let missing_text = (!missing.is_empty()).then(|| {
            format!(
                "Not found: {}. Put the .trl file in {} (or at the path its trailData says, from {}), then paste again.",
                missing.join(", "),
                self.dir.join(trail::TRAILS_DIR).display(),
                self.dir.display()
            )
        });
        if added.is_empty() {
            return self.refuse(missing_text.unwrap_or_else(|| "Nothing to paste.".into()));
        }
        self.save();
        let mut text = format!("Pasted {} trail(s).", added.len());
        let kind = match missing_text {
            Some(missing) => {
                text = format!("{text} {missing}");
                StatusKind::Warning
            }
            None => StatusKind::Info,
        };
        self.set_status(kind, text);
        Ok(added)
    }

    /// Where a pasted trail's `.trl` is: at its `trailData` path in the addon directory, or by
    /// file name in `trails/` or in the addon directory. Returned as a `trailData` path.
    fn find_trl(&self, file: &str) -> Option<String> {
        let file = file.replace('\\', "/");
        let name = file.rsplit('/').next().unwrap_or(&file).to_owned();
        [file.clone(), format!("{}/{name}", trail::TRAILS_DIR), name]
            .into_iter()
            .find(|candidate| {
                files::resolve(&self.dir, candidate).is_some_and(|path| path.is_file())
            })
    }

    // ---- Trail recorder ----

    /// The trail being recorded: its index in `pack.trails`, and whether it's paused.
    pub fn recording(&self) -> Option<(usize, bool)> {
        let recorder = self.recorder.as_ref()?;
        let index = self
            .pack
            .trails
            .iter()
            .position(|t| t.guid == recorder.guid)?;
        Some((index, recorder.paused))
    }

    /// Starts recording a new trail at `pos` on `map_id`, with the current trail category.
    pub fn start_trail(&mut self, map_id: u32, pos: [f32; 3]) -> Result<(), String> {
        let result = self.try_start_trail(map_id, pos);
        // The new trail goes into captures.xml after its .trl is written.
        self.finish(result).inspect(|()| self.save())
    }

    fn try_start_trail(&mut self, map_id: u32, pos: [f32; 3]) -> Result<String, String> {
        if self.recording().is_some() {
            return Err("A trail is already being recorded. Stop it first.".into());
        }
        if map_id == 0 {
            return Err(NOT_ON_MAP.into());
        }
        let category = self.trail_category();
        if category.is_empty() {
            return Err("Set a category for the trail first.".into());
        }
        let file = trail::new_file_name(&self.dir, map_id, &self.pack.trails);
        let mut new = Trail::new(map_id, category, file);
        new.push_point(pos);
        self.pack.ensure_category(&new.category);
        self.recorder = Some(Recorder {
            guid: new.guid.clone(),
            paused: false,
            last_seen: Some(pos),
            unsaved: 0,
        });
        self.pack.trails.push(new);
        let n = self.pack.trails.len();
        Ok(if self.settings.trail_manual {
            format!("Recording trail #{n}: place its points by hand.")
        } else {
            format!(
                "Recording trail #{n}: a point every {} m.",
                fmt_float(self.settings.spacing(), 2)
            )
        })
    }

    /// Records more points on an existing trail, from `pos` (see [`Editor::resume_trail`]).
    pub fn continue_trail(
        &mut self,
        index: usize,
        map_id: u32,
        pos: [f32; 3],
        join: Join,
    ) -> Result<(), String> {
        let before = self.trail_snapshot(index);
        let result = match self.pack.trails.get(index) {
            _ if self.recording().is_some() => {
                Err("A trail is already being recorded. Stop it first.".to_owned())
            }
            None => Err("No such trail.".to_owned()),
            Some(trail) => {
                self.recorder = Some(Recorder {
                    guid: trail.guid.clone(),
                    paused: true,
                    last_seen: None,
                    unsaved: 0,
                });
                let result = self.try_resume_trail(map_id, pos, join);
                if result.is_err() {
                    self.recorder = None;
                }
                result
            }
        };
        // Undoing it removes everything recorded this time.
        if result.is_ok()
            && let Some(before) = before
        {
            self.push_undo(format!("record more of trail #{}", index + 1), before);
        }
        self.finish(result)
    }

    pub fn pause_trail(&mut self) {
        let result = match (self.recording(), self.recorder.as_mut()) {
            (Some((index, false)), Some(recorder)) => {
                recorder.paused = true;
                recorder.last_seen = None;
                Ok(format!("Paused trail #{}.", index + 1))
            }
            (Some(_), _) => Err("The trail is already paused.".into()),
            _ => Err("No trail is being recorded.".into()),
        };
        let _ = self.finish(result);
    }

    /// Resumes the paused trail with the character at `pos`. Far from the trail's last point
    /// (see [`RESUME_JOIN_M`]), this starts a new section.
    pub fn resume_trail(&mut self, map_id: u32, pos: [f32; 3]) -> Result<(), String> {
        let result = self.try_resume_trail(map_id, pos, Join::Auto);
        self.finish(result)
    }

    fn try_resume_trail(
        &mut self,
        map_id: u32,
        pos: [f32; 3],
        join: Join,
    ) -> Result<String, String> {
        let Some((index, paused)) = self.recording() else {
            return Err("No trail to resume.".into());
        };
        if !paused {
            return Err("The trail is already recording.".into());
        }
        let n = index + 1;
        let trail = &mut self.pack.trails[index];
        if map_id == 0 {
            return Err(NOT_ON_MAP.into());
        }
        if map_id != trail.map_id {
            return Err(format!(
                "Trail #{n} is on map {}, you are on map {map_id}. Stop it to record a new trail here.",
                trail.map_id
            ));
        }
        if let Join::Section(section) = join {
            if !trail.move_section_to_end(section) {
                return Err(format!("Trail #{n} has no section {}.", section + 1));
            }
            self.selected_points.clear();
        }
        let trail = &mut self.pack.trails[index];
        let gap = trail.last_point().map(|last| distance(last, pos));
        let new_section = match (join, gap) {
            (_, None) => false,
            (Join::Never, Some(_)) => true,
            (Join::Section(_), Some(_)) => false,
            (Join::Auto, Some(gap)) => gap > RESUME_JOIN_M,
        };
        let text = if new_section {
            trail.break_section();
            trail.push_point(pos);
            format!("Recording trail #{n} again, as a new section.")
        } else {
            // Connect to where the character is now, unless it's already there.
            if gap.is_none_or(|gap| gap >= 0.05) {
                trail.push_point(pos);
            }
            match gap {
                Some(gap) if gap > RESUME_JOIN_M => format!(
                    "Recording trail #{n} again, continuing its line ({} m from its last point).",
                    gap.round()
                ),
                _ => format!("Recording trail #{n} again."),
            }
        };
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.paused = false;
            recorder.last_seen = Some(pos);
        }
        Ok(text)
    }

    /// Finishes the trail being recorded. A trail with fewer than 2 points can't be drawn, so
    /// it's discarded.
    pub fn stop_trail(&mut self) {
        let Some((index, _)) = self.recording() else {
            self.recorder = None;
            return;
        };
        self.recorder = None;
        let n = index + 1;
        let trail = &mut self.pack.trails[index];
        trail.trim_breaks();
        if trail.point_count() < 2 {
            self.remove_trail_at(index);
            self.set_status(
                StatusKind::Warning,
                format!("Discarded trail #{n}: it had fewer than 2 points."),
            );
        } else {
            let text = format!(
                "Finished trail #{n}: {} points, {} m.",
                trail.point_count(),
                trail.length().round()
            );
            self.set_status(StatusKind::Info, text);
            self.save_trail(index);
        }
        self.save();
    }

    /// Adds a point at `pos` to the trail being recorded, paused or not, regardless of the
    /// spacing.
    pub fn add_trail_point(&mut self, map_id: u32, pos: [f32; 3]) -> Result<(), String> {
        let result = self.try_add_trail_point(map_id, pos);
        self.finish(result)
    }

    fn try_add_trail_point(&mut self, map_id: u32, pos: [f32; 3]) -> Result<String, String> {
        let Some((index, _)) = self.recording() else {
            return Err("Start recording a trail first.".into());
        };
        let trail = &mut self.pack.trails[index];
        if map_id == 0 {
            return Err(NOT_ON_MAP.into());
        }
        if map_id != trail.map_id {
            return Err(format!("Trail #{} is on map {}.", index + 1, trail.map_id));
        }
        if trail
            .last_point()
            .is_some_and(|last| distance(last, pos) < 0.05)
        {
            return Err("The trail already ends here.".into());
        }
        trail.push_point(pos);
        Ok(format!(
            "Placed point {} of trail #{}.",
            trail.point_count(),
            index + 1
        ))
    }

    /// Makes the next point of the trail being recorded start a new, unconnected section.
    pub fn break_trail_section(&mut self) {
        let result = match self.recording() {
            None => Err("No trail is being recorded.".to_owned()),
            Some((index, _)) => {
                self.pack.trails[index].break_section();
                Ok(format!(
                    "The next point of trail #{} starts a new section.",
                    index + 1
                ))
            }
        };
        let _ = self.finish(result);
    }

    /// Removes the last point (or pending section break) of the trail being recorded.
    pub fn undo_trail_point(&mut self) {
        let result = match self.recording() {
            None => Err("No trail is being recorded.".into()),
            Some((index, _)) => {
                let trail = &mut self.pack.trails[index];
                if trail.pop_point() {
                    Ok(format!(
                        "Removed the last point of trail #{} ({} left).",
                        index + 1,
                        trail.point_count()
                    ))
                } else {
                    Err("The trail has no points left.".into())
                }
            }
        };
        let _ = self.finish(result);
    }

    /// Records the trail. Call once per frame with the character's map and position (feet), or
    /// `None` outside gameplay. Returns a message when the recorder paused itself, after a map
    /// change or a teleport. In manual mode nothing is recorded.
    pub fn trail_tick(&mut self, here: Option<(u32, [f32; 3])>) -> Option<String> {
        let (index, paused) = self.recording()?;
        let (map_id, pos) = here?;
        if paused {
            return None;
        }
        let recorder = self.recorder.as_mut()?;
        if self.settings.trail_manual {
            // Kept current, so switching to automatic doesn't look like a teleport.
            recorder.last_seen = Some(pos);
            return None;
        }
        let trail = &mut self.pack.trails[index];

        let why = if map_id != trail.map_id {
            Some(format!("you left map {}", trail.map_id))
        } else {
            recorder
                .last_seen
                .map(|seen| distance(seen, pos))
                .filter(|&jump| jump > TELEPORT_M)
                .map(|jump| {
                    format!(
                        "you moved {} m at once (waypoint, portal or respawn?)",
                        jump.round()
                    )
                })
        };
        if let Some(why) = why {
            recorder.paused = true;
            recorder.last_seen = None;
            let text = format!("Trail #{} paused: {why}.", index + 1);
            self.set_status(StatusKind::Warning, text.clone());
            self.save_recording();
            return Some(text);
        }

        recorder.last_seen = Some(pos);
        let far_enough = trail
            .last_point()
            .is_none_or(|last| distance(last, pos) >= self.settings.spacing());
        if far_enough && trail.push_point(pos) {
            recorder.unsaved += 1;
            if recorder.unsaved >= SAVE_EVERY_POINTS {
                self.save_recording();
            }
        }
        None
    }

    // ---- Trail points ----

    /// Saves a trail's `.trl` after the UI changed its points in place, and says so.
    pub fn commit_trail(&mut self, index: usize, text: impl Into<String>) {
        if index < self.pack.trails.len() {
            self.set_status(StatusKind::Info, text);
            self.save_trail(index);
        }
    }

    /// Moves point `point` of trail `index` to `pos` (the character's feet) on `map_id`.
    pub fn move_trail_point(
        &mut self,
        index: usize,
        point: usize,
        map_id: u32,
        pos: [f32; 3],
    ) -> Result<(), String> {
        let before = self.trail_snapshot(index);
        let result = self.check_trail_map(index, map_id).and_then(|()| {
            let trail = &mut self.pack.trails[index];
            match trail.points.get_mut(point) {
                Some(p) if !trail::is_break(*p) => {
                    *p = pos;
                    Ok(format!("Moved a point of trail #{}.", index + 1))
                }
                _ => Err("No such point.".to_owned()),
            }
        });
        self.finish_trail_edit(index, result, before, "move a trail point")
    }

    /// Adds a point at `pos` right after point `point` of trail `index`. Returns its index.
    pub fn insert_trail_point(
        &mut self,
        index: usize,
        point: usize,
        map_id: u32,
        pos: [f32; 3],
    ) -> Result<usize, String> {
        let before = self.trail_snapshot(index);
        let mut inserted = None;
        let result = self.check_trail_map(index, map_id).and_then(|()| {
            inserted = self.pack.trails[index].insert_point(point, pos);
            match inserted {
                Some(_) => Ok(format!("Added a point to trail #{}.", index + 1)),
                None => Err("No such point.".to_owned()),
            }
        });
        self.finish_trail_edit(index, result, before, "add a trail point")
            .map(|()| inserted.unwrap_or_default())
    }

    /// Deletes points of trail `index`.
    pub fn remove_trail_points(
        &mut self,
        index: usize,
        points: &BTreeSet<usize>,
    ) -> Result<(), String> {
        let before = self.trail_snapshot(index);
        let points: Vec<usize> = points.iter().copied().collect();
        let result = match self.pack.trails.get_mut(index) {
            Some(trail) => match trail.remove_points(&points) {
                0 => Err("No such point.".to_owned()),
                n => Ok(format!(
                    "Deleted {n} point(s) of trail #{} ({} left).",
                    index + 1,
                    trail.point_count()
                )),
            },
            None => Err("No such trail.".to_owned()),
        };
        if result.is_ok() {
            self.selected_points.clear();
        }
        self.finish_trail_edit(index, result, before, "delete trail points")
    }

    /// Moves points of trail `index` by `delta` metres, in place: call
    /// [`Editor::checkpoint_trail`] before the first move and [`Editor::commit_trail`] after
    /// the last.
    pub fn offset_trail_points(&mut self, index: usize, points: &BTreeSet<usize>, delta: [f32; 3]) {
        let Some(trail) = self.pack.trails.get_mut(index) else {
            return;
        };
        for &i in points {
            if let Some(p) = trail.points.get_mut(i)
                && !trail::is_break(*p)
            {
                *p = add(*p, delta);
            }
        }
    }

    /// Moves points of trail `index` together so that the first one is at `pos` (the
    /// character's feet), keeping how they're laid out.
    pub fn move_trail_points_here(
        &mut self,
        index: usize,
        points: &BTreeSet<usize>,
        map_id: u32,
        pos: [f32; 3],
    ) -> Result<(), String> {
        let before = self.trail_snapshot(index);
        let result = self.check_trail_map(index, map_id).and_then(|()| {
            let trail = &self.pack.trails[index];
            let first = points
                .iter()
                .find(|&&i| trail.points.get(i).is_some_and(|&p| !trail::is_break(p)))
                .ok_or("No such point.")?;
            Ok(sub(pos, trail.points[*first]))
        });
        let result = result.map(|delta| {
            self.offset_trail_points(index, points, delta);
            format!(
                "Moved {} point(s) of trail #{} here.",
                points.len(),
                index + 1
            )
        });
        self.finish_trail_edit(index, result, before, "move trail points")
    }

    /// Inserts `points` into trail `index`, after point `after` (else at the end). With
    /// `here`, they move together so that the first one is at that position. Selects them.
    pub fn paste_trail_points(
        &mut self,
        index: usize,
        after: Option<usize>,
        mut points: Vec<[f32; 3]>,
        here: Option<(u32, [f32; 3])>,
    ) -> Result<(), String> {
        let before = self.trail_snapshot(index);
        let check = match here {
            Some((map_id, _)) => self.check_trail_map(index, map_id),
            None if index < self.pack.trails.len() => Ok(()),
            None => Err("No such trail.".to_owned()),
        };
        let result = check.and_then(|()| {
            points.retain(|&p| !trail::is_break(p));
            let first = *points.first().ok_or("Nothing to paste.")?;
            if let Some((_, pos)) = here {
                let delta = sub(pos, first);
                points.iter_mut().for_each(|p| *p = add(*p, delta));
            }
            let trail = &mut self.pack.trails[index];
            let at = after
                .filter(|&a| a < trail.points.len())
                .map_or(trail.points.len(), |a| a + 1);
            let n = points.len();
            trail.points.splice(at..at, points);
            self.selected_points = (at..at + n).collect();
            Ok(format!("Pasted {n} point(s) into trail #{}.", index + 1))
        });
        self.finish_trail_edit(index, result, before, "paste trail points")
    }

    fn check_trail_map(&self, index: usize, map_id: u32) -> Result<(), String> {
        let trail = self.pack.trails.get(index).ok_or("No such trail.")?;
        if map_id == 0 {
            Err(NOT_ON_MAP.into())
        } else if map_id != trail.map_id {
            Err(format!(
                "Trail #{} is on map {}, you are on map {map_id}.",
                index + 1,
                trail.map_id
            ))
        } else {
            Ok(())
        }
    }

    /// Shows the outcome of a point edit. On success, saves the trail and makes the edit
    /// undoable (`before` is the trail's state before it).
    fn finish_trail_edit(
        &mut self,
        index: usize,
        result: Result<String, String>,
        before: Option<Snapshot>,
        label: &str,
    ) -> Result<(), String> {
        match result {
            Ok(text) => {
                if let Some(before) = before {
                    self.push_undo(format!("{label} (trail #{})", index + 1), before);
                }
                self.commit_trail(index, text);
                Ok(())
            }
            Err(text) => self.refuse(text),
        }
    }

    /// Deletes a trail and its `.trl` file (only if the editor created it). The trail being
    /// recorded can't be deleted.
    pub fn remove_trail(&mut self, index: usize) {
        if index >= self.pack.trails.len() {
            return;
        }
        if self.recording().is_some_and(|(i, _)| i == index) {
            self.set_status(
                StatusKind::Warning,
                "Stop recording before deleting this trail.",
            );
            return;
        }
        self.set_status(StatusKind::Info, format!("Deleted trail #{}", index + 1));
        self.remove_trail_at(index);
        self.save();
    }

    fn remove_trail_at(&mut self, index: usize) {
        let trail = self.pack.trails.remove(index);
        if self.selection == Some(Selection::Trail(index)) {
            self.selected_points.clear();
        }
        if let Some(Selection::Trail(selected)) = self.selection {
            self.selection = match selected.cmp(&index) {
                std::cmp::Ordering::Less => Some(Selection::Trail(selected)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(Selection::Trail(selected - 1)),
            };
        }
        self.delete_trail_file(&trail);
    }

    // ---- Archives ----

    /// Writes the pack to `export/<name>.<extension>` (`zip` or `taco`): the XML, then the
    /// images, `.trl` and `.lua` files it uses. Says how it went in the status.
    pub fn export(&mut self, name: &str, extension: &str) {
        self.flush();
        self.save();
        let name = archive::safe_file_name(name);
        let out = self
            .dir
            .join(archive::EXPORT_DIR)
            .join(format!("{name}.{extension}"));
        match archive::export(&self.dir, &self.pack, &out, &format!("{name}.xml")) {
            Ok(report) if report.missing.is_empty() => self.set_status(
                StatusKind::Info,
                format!(
                    "Exported {} (the XML and {} file(s)).",
                    out.display(),
                    report.files.len()
                ),
            ),
            Ok(report) => self.set_status(
                StatusKind::Warning,
                format!(
                    "Exported {}, without {} file(s) the pack uses but the folder doesn't have: {}.",
                    out.display(),
                    report.missing.len(),
                    report.missing.join(", ")
                ),
            ),
            Err(e) => self.set_status(StatusKind::Error, format!("Export failed: {e}")),
        }
    }

    /// Imports the archive `file` of `import/`: in place of the pack (`merge` false) or added
    /// to it. With `backup`, the current pack is first saved to `export/backup-<date>.zip`.
    /// Says how it went in the status.
    pub fn import(&mut self, file: &str, merge: bool, backup: bool) {
        match self.try_import(file, merge, backup) {
            Ok((kind, text)) => self.set_status(kind, text),
            Err(e) => self.set_status(StatusKind::Error, format!("Import failed: {e}")),
        }
    }

    fn try_import(
        &mut self,
        file: &str,
        merge: bool,
        backup: bool,
    ) -> Result<(StatusKind, String), String> {
        if self.recorder.is_some() {
            return Err("stop the trail being recorded first.".into());
        }
        if !self.persist {
            return Err(format!(
                "{CAPTURES_FILE} couldn't be read, so the folder is left alone. Reload the addon first."
            ));
        }
        let path = files::resolve(&self.dir.join(archive::IMPORT_DIR), file)
            .ok_or_else(|| format!("{file} isn't a file name."))?;
        // Read and check it all before changing anything.
        let import = archive::read_archive(&path)?;
        let mut notes = import.warnings.clone();
        let xml_files = import.xml_files;
        let (categories, pois, trails) = (
            import.pack.categories.len(),
            import.pack.pois.len(),
            import.pack.trails.len(),
        );

        self.flush();
        self.save();
        let backup = if !backup || archive::folder_files(&self.dir).is_empty() {
            None
        } else {
            let out = self
                .dir
                .join(archive::EXPORT_DIR)
                .join(archive::backup_name());
            archive::backup(&self.dir, &out).map_err(|e| {
                format!("couldn't back up the current pack ({e}), so nothing was changed.")
            })?;
            Some(out)
        };
        let halfway = |e: String| match &backup {
            Some(b) => format!(
                "{e}. The folder may be half changed: the previous pack is in {}, import it to get it back.",
                b.display()
            ),
            None => e,
        };
        let (written, new_guids) = if merge {
            import.merge_into(&self.dir, &self.pack).map_err(halfway)?
        } else {
            (import.replace_folder(&self.dir).map_err(halfway)?, 0)
        };

        let mut fresh = Editor::open(&self.dir);
        fresh.settings = self.settings.clone();
        fresh.saved_settings = self.saved_settings.clone();
        let reopened = fresh.status.take();
        *self = fresh;

        let mut text = format!(
            "{} {file} ({xml_files} XML file(s)): {categories} top-level categories, {pois} markers, {trails} trails, {} other files.",
            if merge { "Added" } else { "Imported" },
            written.files
        );
        match &backup {
            Some(b) => text.push_str(&format!(" The previous pack is in {}.", b.display())),
            None => text.push_str(" No backup was made."),
        }
        if !written.renamed.is_empty() {
            notes.push(format!(
                "{} file(s) differ from yours at the same path, so they were added under a new name ({}).",
                written.renamed.len(),
                short_list(written.renamed.iter().map(|(_, new)| new.as_str()))
            ));
        }
        if !written.kept.is_empty() {
            notes.push(format!(
                "Kept your version of {} (scripts and {} files are never overwritten: merge them by hand).",
                short_list(written.kept.iter().map(String::as_str)),
                archive::LIBRARY_DIR
            ));
        }
        if new_guids > 0 {
            notes.push(format!(
                "{new_guids} imported item(s) had a GUID the pack already used and got a new one."
            ));
        }
        notes.extend(reopened.map(|s| s.text));
        let kind = if notes.is_empty() {
            StatusKind::Info
        } else {
            StatusKind::Warning
        };
        for note in notes {
            text.push(' ');
            text.push_str(&note);
        }
        Ok((kind, text))
    }

    // ---- Saving ----

    /// Writes whatever isn't on disk yet. Call before dropping the editor.
    pub fn flush(&mut self) {
        if self.recorder.as_ref().is_some_and(|r| r.unsaved > 0) {
            self.save_recording();
        }
        self.save_settings_if_changed();
    }

    /// Call after the UI may have changed `settings`.
    pub fn save_settings_if_changed(&mut self) {
        if self.settings == self.saved_settings {
            return;
        }
        // Remember even if the write fails, or it would be retried every frame.
        self.saved_settings = self.settings.clone();
        let path = self.dir.join(SETTINGS_FILE);
        if let Err(e) = self.settings.save(&path) {
            self.set_status(
                StatusKind::Error,
                format!("Couldn't save {}: {e}", path.display()),
            );
        }
    }

    /// Shows the outcome of a trail action. On success, saves the trail's `.trl`.
    fn finish(&mut self, result: Result<String, String>) -> Result<(), String> {
        match result {
            Ok(text) => {
                self.set_status(StatusKind::Info, text);
                self.save_recording();
                Ok(())
            }
            Err(text) => self.refuse(text),
        }
    }

    fn save_recording(&mut self) {
        if let Some((index, _)) = self.recording() {
            self.save_trail(index);
        }
    }

    fn save_trail(&mut self, index: usize) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.unsaved = 0;
        }
        if !self.persist {
            return;
        }
        let trail = &self.pack.trails[index];
        if let Err(e) = trail::save(&self.dir, trail) {
            let text = format!("Couldn't save {}: {e}", trail.file);
            self.set_status(StatusKind::Error, text);
        }
    }

    fn delete_trail_file(&mut self, trail: &Trail) {
        if !self.persist || !trail::is_own_file(&trail.file) {
            return;
        }
        let Some(path) = files::resolve(&self.dir, &trail.file) else {
            return;
        };
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                self.set_status(
                    StatusKind::Warning,
                    format!("Couldn't delete {}: {e}", path.display()),
                );
            }
            _ => {}
        }
    }

    fn save(&mut self) {
        if !self.persist {
            return;
        }
        let file = self.file();
        if let Err(e) = pack::save_file(&file, &self.pack) {
            self.set_status(
                StatusKind::Error,
                format!("Couldn't save {}: {e}", file.display()),
            );
        }
    }
}

/// The first few of `items`, joined, with "and N more".
fn short_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items: Vec<&str> = items.collect();
    let shown = items.iter().take(4).copied().collect::<Vec<_>>().join(", ");
    match items.len() {
        n if n > 4 => format!("{shown} and {} more", n - 4),
        _ => shown,
    }
}

/// The last non-empty category of a list, to restore a category box.
fn last_category<'a>(
    categories: impl DoubleEndedIterator<Item = &'a String>,
    default: &str,
) -> String {
    categories
        .rev()
        .find(|c| !c.is_empty())
        .cloned()
        .unwrap_or_else(|| default.into())
}

/// "marker #3" or "4 markers".
fn markers_label(indices: &[usize]) -> String {
    match indices {
        [one] => format!("marker #{}", one + 1),
        many => format!("{} markers", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attrs;

    fn temp_dir() -> PathBuf {
        let name = crate::guid::new_guid().replace(['/', '+', '='], "");
        let dir = std::env::temp_dir().join(format!("pathing_editor_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn renaming_a_category_and_undoing_it() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.category_input = "peak.jps".into();
        editor.capture(50, [1.0, 2.0, 3.0]).unwrap();
        editor.category_input = "eli.chests".into();
        editor.capture(50, [9.0, 2.0, 3.0]).unwrap();
        editor.category_input = "peak.jps".into();
        editor.settings.set_hidden("peak.jps", true);
        fs::write(dir.join("pack.lua"), "World:CategoryByType(\"peak.jps\")").unwrap();
        editor.selection = Some(Selection::Category("peak.jps".into()));

        assert_eq!(editor.rename_category("peak", "Eli").unwrap(), "eli");
        assert_eq!(editor.pack.pois[0].category, "eli.jps");
        assert!(editor.pack.category("peak").is_none());
        assert_eq!(
            editor.selection,
            Some(Selection::Category("eli.jps".into()))
        );
        assert_eq!(editor.category_input, "eli.jps");
        assert!(editor.settings.is_hidden_itself("eli.jps"));
        let status = editor.status.clone().unwrap();
        assert_eq!(status.kind, StatusKind::Warning, "{}", status.text);
        assert!(status.text.contains("pack.lua"), "{}", status.text);
        assert_eq!(Editor::open(&dir).pack.pois[0].category, "eli.jps", "saved");

        editor.undo();
        assert_eq!(editor.pack.pois[0].category, "peak.jps");
        assert!(editor.pack.category("peak.jps").is_some());
        assert_eq!(editor.selection, None, "eli.jps no longer exists");
        editor.redo();
        assert_eq!(editor.pack.pois[0].category, "eli.jps");
        assert!(editor.pack.category("peak").is_none());
        assert!(editor.rename_category("eli", " ").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_and_import() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.preview = !editor.settings.preview;
        editor.capture(50, [1.0, 2.0, 3.0]).unwrap();
        editor.export("My Pack", "taco");
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Info);
        let exported = dir.join(archive::EXPORT_DIR).join("My Pack.taco");
        assert!(exported.is_file());

        // Another pack is opened in its place...
        editor.capture(50, [9.0, 9.0, 9.0]).unwrap();
        fs::create_dir_all(dir.join(archive::IMPORT_DIR)).unwrap();
        fs::copy(&exported, dir.join(archive::IMPORT_DIR).join("a.taco")).unwrap();
        let settings = editor.settings.clone();
        editor.import("a.taco", false, true);
        let status = editor.status.clone().unwrap();
        assert_eq!(status.kind, StatusKind::Info, "{}", status.text);
        assert!(status.text.contains("backup-"), "{}", status.text);
        assert_eq!(editor.pack.pois.len(), 1, "replaced, not merged");
        assert_eq!(editor.pack.pois[0].pos, [1.0, 2.0, 3.0]);
        assert_eq!(editor.settings, settings, "settings kept");
        assert_eq!(Editor::open(&dir).pack, editor.pack, "saved");

        // ...and the backup holds the one it replaced.
        let backup = fs::read_dir(dir.join(archive::EXPORT_DIR))
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("backup-"))
            .unwrap();
        fs::copy(backup.path(), dir.join(archive::IMPORT_DIR).join("b.zip")).unwrap();
        editor.import("b.zip", false, true);
        assert_eq!(editor.pack.pois.len(), 2);

        // Added to it, without a backup.
        let backups = |dir: &Path| fs::read_dir(dir.join(archive::EXPORT_DIR)).unwrap().count();
        let before = backups(&dir);
        editor.import("a.taco", true, false);
        assert_eq!(editor.pack.pois.len(), 3, "merged");
        assert!(editor.status.as_ref().unwrap().text.contains("No backup"));
        assert_eq!(backups(&dir), before);

        editor.import("../captures.xml", false, true);
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Error);
        editor.import("missing.zip", false, true);
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Error);
        assert_eq!(editor.pack.pois.len(), 3, "a failed import changes nothing");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn capture_saves_and_reopens() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.category_input, DEFAULT_CATEGORY);

        editor.category_input = "My Pack.Chests".into();
        assert_eq!(editor.capture(50, [1.0, 2.0, 3.0]), Ok(0));
        assert_eq!(editor.pack.pois[0].category, "my_pack.chests");
        assert!(editor.pack.category("my_pack.chests").is_some());

        let reopened = Editor::open(&dir);
        assert_eq!(reopened.pack, editor.pack);
        assert_eq!(
            reopened.category_input, "my_pack.chests",
            "category restored from last capture"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn duplicate_guard() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.capture(50, [0.0, 0.0, 0.0]).unwrap();

        // Same category, same map, 1 m away: refused.
        assert!(editor.capture(50, [1.0, 0.0, 0.0]).is_err());
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Warning);
        // Other map, other category, or far enough: fine.
        assert!(editor.capture(51, [1.0, 0.0, 0.0]).is_ok());
        assert!(editor.capture(50, [2.5, 0.0, 0.0]).is_ok());
        editor.category_input = "other".into();
        assert!(editor.capture(50, [1.0, 0.0, 0.0]).is_ok());
        // Guard off.
        editor.settings.skip_near_duplicates = false;
        assert!(editor.capture(50, [1.0, 0.0, 0.0]).is_ok());
        assert_eq!(editor.pack.pois.len(), 5);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn raised_markers() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.capture(50, [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(editor.pack.pois[0].pos, [1.0, 2.0, 3.0]);

        editor.settings.raise_markers = true;
        assert_eq!(editor.marker_pos([1.0, 2.0, 3.0]), [1.0, 3.5, 3.0]);
        // 1.5 m above the first one: still a duplicate.
        assert!(editor.capture(50, [1.0, 2.0, 3.0]).is_err());
        editor.capture(50, [10.0, 2.0, 3.0]).unwrap();
        assert_eq!(editor.pack.pois[1].pos, [10.0, 3.5, 3.0]);
        // Moving uses the raise too.
        editor.move_poi(0, 51, [5.0, 5.0, 5.0]).unwrap();
        assert_eq!(
            (editor.pack.pois[0].map_id, editor.pack.pois[0].pos),
            (51, [5.0, 6.5, 5.0])
        );
        assert_eq!(Editor::open(&dir).pack.pois[0].map_id, 51, "saved");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_persist() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.settings, Settings::default());
        editor.settings.raise_markers = true;
        editor.settings.trail_spacing = 2.5;
        editor.save_settings_if_changed();
        assert_eq!(Editor::open(&dir).settings, editor.settings);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_without_map_or_category() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        assert!(editor.capture(0, [0.0; 3]).is_err());
        assert!(editor.start_trail(0, [1.0; 3]).is_err());
        editor.category_input = " . ".into();
        editor.trail_category_input = String::new();
        assert!(editor.capture(50, [0.0; 3]).is_err());
        assert!(editor.start_trail(50, [1.0; 3]).is_err());
        assert!(editor.pack.pois.is_empty() && editor.pack.trails.is_empty());
        assert!(
            !dir.join(CAPTURES_FILE).exists(),
            "nothing captured, nothing written"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remove_and_clear() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.skip_near_duplicates = false;
        for _ in 0..3 {
            editor.capture(50, [0.0; 3]).unwrap();
        }
        let second = editor.pack.pois[1].guid.clone();
        editor.selection = Some(Selection::poi(2));
        editor.remove_poi(0);
        assert_eq!(editor.pack.pois[0].guid, second);
        assert_eq!(
            editor.selection,
            Some(Selection::poi(1)),
            "selection follows"
        );
        editor.remove_poi(99); // out of range: ignored
        assert_eq!(Editor::open(&dir).pack.pois.len(), 2);
        editor.clear_pois();
        assert_eq!(editor.selection, None);
        assert!(Editor::open(&dir).pack.pois.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn categories() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.add_category("", "My Pack"), Ok("my_pack".into()));
        assert_eq!(
            editor.add_category("my_pack", "Jumping Puzzles"),
            Ok("my_pack.jumping_puzzles".into())
        );
        assert!(editor.add_category("my_pack", "jumping_puzzles").is_err());
        assert!(editor.add_category("my_pack", " ").is_err());

        editor
            .add_poi("my_pack.jumping_puzzles", 50, [1.0; 3])
            .unwrap();
        attrs::set(
            &mut editor.pack.category_mut("my_pack").unwrap().attrs,
            "DisplayName",
            "My Pack",
        );
        editor.commit("renamed");

        editor.selection = Some(Selection::Category("my_pack.jumping_puzzles".into()));
        assert!(editor.remove_category("my_pack").is_err(), "has a marker");
        editor.set_poi_category(0, "Other").unwrap();
        assert!(editor.pack.category("other").is_some());
        editor.remove_category("my_pack").unwrap();
        assert_eq!(editor.selection, None);

        let reopened = Editor::open(&dir);
        assert!(reopened.pack.category("my_pack").is_none());
        assert_eq!(reopened.pack.pois[0].category, "other");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_and_redo_markers() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.undo_label(), None);
        editor.capture(50, [1.0; 3]).unwrap();
        editor.move_poi(0, 50, [5.0, 1.0, 1.0]).unwrap();
        editor.remove_poi(0);
        assert!(editor.pack.pois.is_empty());

        editor.undo();
        assert_eq!(editor.pack.pois[0].pos, [5.0, 1.0, 1.0], "delete undone");
        editor.undo();
        assert_eq!(editor.pack.pois[0].pos, [1.0; 3], "move undone");
        assert_eq!(Editor::open(&dir).pack.pois[0].pos, [1.0; 3], "and saved");
        editor.undo();
        assert!(editor.pack.pois.is_empty(), "capture undone");
        editor.undo();
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Warning);

        editor.redo();
        editor.redo();
        assert_eq!(editor.pack.pois[0].pos, [5.0, 1.0, 1.0]);
        assert_eq!(editor.redo_label(), Some("delete marker #1"));
        // A new change drops what could be redone.
        editor.capture(50, [20.0; 3]).unwrap();
        assert_eq!(editor.redo_label(), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn groups_of_markers() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.skip_near_duplicates = false;
        for x in [0.0, 10.0, 20.0, 30.0] {
            editor.capture(50, [x, 0.0, 0.0]).unwrap();
        }
        let group = BTreeSet::from([1, 3]);
        editor
            .move_pois_here(&group, 51, [100.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(editor.pack.pois[1].pos, [100.0, 0.0, 0.0]);
        assert_eq!(editor.pack.pois[3].pos, [120.0, 0.0, 0.0], "layout kept");
        assert_eq!(editor.pack.pois[3].map_id, 51);

        editor.set_pois_category(&group, "other").unwrap();
        assert_eq!(editor.pack.pois[3].category, "other");

        editor.selection = Some(Selection::Pois(BTreeSet::from([0, 2, 3])));
        editor.remove_pois(&group);
        assert_eq!(editor.pack.pois.len(), 2);
        assert_eq!(
            editor.selection,
            Some(Selection::Pois(BTreeSet::from([0, 1]))),
            "the survivors stay selected"
        );
        editor.undo();
        assert_eq!(editor.pack.pois.len(), 4);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paste_markers() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.raise_markers = true;
        let copied = vec![
            Poi::new(50, [1.0, 1.0, 1.0], "a"),
            Poi::new(50, [3.0, 1.0, 1.0], "a"),
        ];
        // In place: same map and positions, new GUIDs.
        let added = editor.paste_pois(copied.clone(), None, None).unwrap();
        assert_eq!(added, vec![0, 1]);
        assert_eq!(editor.pack.pois[1].pos, [3.0, 1.0, 1.0]);
        assert_ne!(editor.pack.pois[0].guid, copied[0].guid);
        assert!(editor.pack.category("a").is_some());
        // Here, into a category: the first lands where a new marker would.
        editor
            .paste_pois(copied, Some((60, [10.0, 0.0, 0.0])), Some("b"))
            .unwrap();
        assert_eq!(editor.pack.pois[2].pos, [10.0, 1.5, 0.0]);
        assert_eq!(editor.pack.pois[3].pos, [12.0, 1.5, 0.0]);
        assert_eq!(
            (
                editor.pack.pois[3].map_id,
                editor.pack.pois[3].category.as_str()
            ),
            (60, "b")
        );
        assert_eq!(
            editor.selection,
            Some(Selection::Pois(BTreeSet::from([2, 3])))
        );
        assert!(editor.paste_pois(Vec::new(), None, None).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn group_and_undo_trail_points() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.start_trail(50, [1.0; 3]).unwrap();
        walk(&mut editor, 50, [1.0; 3], 4);
        editor.stop_trail();
        let original = editor.pack.trails[0].points.clone(); // x = 1..=5

        let group = BTreeSet::from([1, 2]);
        editor
            .move_trail_points_here(0, &group, 50, [2.0, 9.0, 1.0])
            .unwrap();
        assert_eq!(editor.pack.trails[0].points[2], [3.0, 9.0, 1.0]);
        editor
            .remove_trail_points(0, &BTreeSet::from([0, 4]))
            .unwrap();
        assert_eq!(editor.pack.trails[0].point_count(), 3);
        editor
            .paste_trail_points(0, Some(0), vec![[7.0; 3], [8.0; 3]], None)
            .unwrap();
        assert_eq!(editor.pack.trails[0].points[1], [7.0; 3]);
        assert_eq!(editor.selected_points, BTreeSet::from([1, 2]));
        editor
            .paste_trail_points(0, None, vec![[1.0; 3]], Some((50, [40.0; 3])))
            .unwrap();
        assert_eq!(editor.pack.trails[0].last_point(), Some([40.0; 3]));

        for _ in 0..4 {
            editor.undo();
        }
        assert_eq!(editor.pack.trails[0].points, original);
        assert_eq!(Editor::open(&dir).pack.trails[0].points, original, "saved");

        // Recording more is one step, and can't be undone while recording.
        editor
            .continue_trail(0, 50, [5.0, 1.0, 1.0], Join::Auto)
            .unwrap();
        walk(&mut editor, 50, [5.0, 1.0, 1.0], 3);
        editor.undo();
        assert_eq!(editor.pack.trails[0].point_count(), 8, "refused");
        editor.stop_trail();
        editor.undo();
        assert_eq!(editor.pack.trails[0].points, original);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn continue_a_chosen_section() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.trail_manual = true;
        editor.start_trail(50, [1.0, 0.0, 0.0]).unwrap();
        editor.add_trail_point(50, [2.0, 0.0, 0.0]).unwrap();
        editor.break_trail_section();
        editor.add_trail_point(50, [50.0, 0.0, 0.0]).unwrap();
        editor.add_trail_point(50, [51.0, 0.0, 0.0]).unwrap();
        editor.stop_trail();

        // Continue the first section from far away: a line from its end, no new section.
        editor
            .continue_trail(0, 50, [10.0, 0.0, 0.0], Join::Section(0))
            .unwrap();
        editor.stop_trail();
        let trail = &editor.pack.trails[0];
        assert_eq!(trail.section_count(), 2);
        assert_eq!(
            trail.sections().last().unwrap(),
            &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [10.0, 0.0, 0.0]]
        );
        assert!(
            editor
                .continue_trail(0, 50, [1.0; 3], Join::Section(5))
                .is_err(),
            "no such section"
        );
        assert_eq!(editor.recording(), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paste_trails_from_another_pack() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        // The other pack's trails: one copied at its path, one dropped in trails/.
        let points = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        // (Windows matches the path's case loosely, the test host doesn't.)
        fs::create_dir_all(dir.join("Data")).unwrap();
        fs::create_dir_all(dir.join("trails")).unwrap();
        fs::write(
            dir.join("Data").join("a.trl"),
            trail::encode_trl(15, &points),
        )
        .unwrap();
        fs::write(
            dir.join("trails").join("b.trl"),
            trail::encode_trl(15, &points),
        )
        .unwrap();
        let line = |file: &str| {
            let mut t = Trail::new(0, "pack.trails", file);
            t.attrs = vec![("texture".into(), "Data/arrow.png".into())];
            t
        };
        let originals = vec![
            line("Data\\a.trl"),
            line("Other/Dir/b.trl"),
            line("gone.trl"),
        ];

        let added = editor
            .paste_trails(originals.clone(), None, Some("mine"))
            .unwrap();
        assert_eq!(added, vec![0, 1]);
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Warning);
        assert!(editor.status.as_ref().unwrap().text.contains("gone.trl"));
        let pasted = &editor.pack.trails[0];
        assert_eq!((pasted.map_id, &pasted.points), (15, &points));
        assert_eq!(pasted.category, "mine");
        assert_eq!(pasted.attrs, originals[0].attrs, "attributes kept");
        assert_ne!(pasted.guid, originals[0].guid);
        assert_eq!(pasted.file, "trails/map15_1.trl", "its own file");
        assert_eq!(editor.pack.trails[1].file, "trails/map15_2.trl");

        // Deleting the copy leaves the original file alone.
        editor.remove_trail(0);
        assert!(dir.join("Data").join("a.trl").exists());
        assert_eq!(Editor::open(&dir).pack.trails.len(), 1, "saved");

        // Here: moved so the first point is at the position, on that map.
        editor
            .paste_trails(vec![line("Data/a.trl")], Some((50, [10.0, 0.0, 0.0])), None)
            .unwrap();
        let moved = editor.pack.trails.last().unwrap();
        assert_eq!(moved.map_id, 50);
        assert_eq!(moved.points, vec![[10.0, 0.0, 0.0], [13.0, 3.0, 3.0]]);
        assert!(
            editor
                .paste_trails(vec![line("gone.trl")], None, None)
                .is_err()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Walks `steps` metres east from `from`, one metre per frame.
    fn walk(editor: &mut Editor, map_id: u32, from: [f32; 3], steps: usize) -> [f32; 3] {
        let mut pos = from;
        for _ in 0..steps {
            pos[0] += 1.0;
            assert_eq!(editor.trail_tick(Some((map_id, pos))), None);
        }
        pos
    }

    #[test]
    fn record_a_trail() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.trail_category_input = "My Pack.Route".into();
        let start = [10.0, 0.0, 0.0];
        editor.start_trail(50, start).unwrap();
        assert_eq!(editor.recording(), Some((0, false)));
        assert!(editor.start_trail(50, start).is_err(), "one at a time");

        // Standing still adds nothing, walking adds a point per metre (default spacing).
        assert_eq!(editor.trail_tick(Some((50, start))), None);
        let mut pos = walk(&mut editor, 50, start, 10);
        assert_eq!(editor.pack.trails[0].point_count(), 11);
        // Not in game: nothing happens.
        assert_eq!(editor.trail_tick(None), None);

        // Pause, walk a little, resume: same section.
        editor.pause_trail();
        assert_eq!(editor.recording(), Some((0, true)));
        assert_eq!(editor.trail_tick(Some((50, [500.0, 0.0, 0.0]))), None);
        pos[0] += 3.0;
        editor.resume_trail(50, pos).unwrap();
        assert_eq!(editor.pack.trails[0].section_count(), 1);
        pos = walk(&mut editor, 50, pos, 2);

        // Pause, resume far away: new section starting there.
        editor.pause_trail();
        pos[2] += 100.0;
        editor.resume_trail(50, pos).unwrap();
        assert_eq!(editor.pack.trails[0].section_count(), 2);
        assert_eq!(editor.pack.trails[0].last_point(), Some(pos));
        walk(&mut editor, 50, pos, 5);

        // Manual point and undo.
        let count = editor.pack.trails[0].point_count();
        let last = editor.pack.trails[0].last_point().unwrap();
        assert!(editor.add_trail_point(50, last).is_err());
        editor.add_trail_point(50, [0.5, 0.5, 0.5]).unwrap();
        editor.undo_trail_point();
        assert_eq!(editor.pack.trails[0].point_count(), count);

        editor.stop_trail();
        assert_eq!(editor.recording(), None);
        let recorded = editor.pack.trails[0].clone();
        assert_eq!(recorded.category, "my_pack.route");
        assert_eq!(recorded.file, "trails/map50_1.trl");

        // The trail is in captures.xml and its points in the .trl.
        let reopened = Editor::open(&dir);
        assert_eq!(reopened.pack.trails, vec![recorded]);
        assert_eq!(reopened.trail_category_input, "my_pack.route");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn manual_trail() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.settings.trail_manual = true;
        editor.start_trail(50, [1.0; 3]).unwrap();
        // Moving records nothing, not even a "teleport".
        walk(&mut editor, 50, [1.0; 3], 10);
        assert_eq!(editor.trail_tick(Some((50, [900.0; 3]))), None);
        assert_eq!(editor.pack.trails[0].point_count(), 1);

        editor.add_trail_point(50, [5.0, 1.0, 1.0]).unwrap();
        editor.break_trail_section();
        editor.add_trail_point(50, [50.0, 1.0, 1.0]).unwrap();
        editor.add_trail_point(50, [60.0, 1.0, 1.0]).unwrap();
        assert_eq!(editor.pack.trails[0].section_count(), 2);
        assert_eq!(editor.pack.trails[0].length(), 4.0 + 10.0);

        // A pending break is dropped when stopping.
        editor.break_trail_section();
        editor.stop_trail();
        assert_eq!(editor.pack.trails[0].point_count(), 4);
        assert_eq!(editor.pack.trails[0].points.len(), 5);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn continue_a_trail() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.start_trail(50, [1.0; 3]).unwrap();
        let end = walk(&mut editor, 50, [1.0; 3], 3);
        editor.stop_trail();

        assert!(
            editor.continue_trail(0, 51, end, Join::Auto).is_err(),
            "other map"
        );
        assert_eq!(editor.recording(), None);
        editor.continue_trail(0, 50, end, Join::Auto).unwrap();
        assert_eq!(editor.recording(), Some((0, false)));
        walk(&mut editor, 50, end, 3);
        editor.stop_trail();
        assert_eq!(editor.pack.trails[0].point_count(), 7);
        assert_eq!(editor.pack.trails[0].section_count(), 1);

        // Far from the end: a new section, unless asked to continue the line.
        let far = [500.0, 1.0, 1.0];
        editor.continue_trail(0, 50, far, Join::Section(0)).unwrap();
        editor.stop_trail();
        assert_eq!(editor.pack.trails[0].section_count(), 1);
        assert_eq!(editor.pack.trails[0].last_point(), Some(far));
        editor
            .continue_trail(0, 50, [900.0, 1.0, 1.0], Join::Auto)
            .unwrap();
        editor.stop_trail();
        assert_eq!(editor.pack.trails[0].section_count(), 2);
        // Never: a new section even right at the end.
        editor
            .continue_trail(0, 50, [900.0, 1.0, 1.0], Join::Never)
            .unwrap();
        editor.trail_tick(Some((50, [905.0, 1.0, 1.0])));
        editor.stop_trail();
        assert_eq!(editor.pack.trails[0].section_count(), 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn edit_trail_points() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.start_trail(50, [1.0; 3]).unwrap();
        walk(&mut editor, 50, [1.0; 3], 3);
        editor.stop_trail();
        // Points at x = 1, 2, 3, 4.

        editor.move_trail_point(0, 1, 50, [2.0, 5.0, 1.0]).unwrap();
        assert!(
            editor.move_trail_point(0, 1, 51, [0.0; 3]).is_err(),
            "other map"
        );
        assert!(editor.move_trail_point(0, 99, 50, [0.0; 3]).is_err());
        assert_eq!(editor.insert_trail_point(0, 1, 50, [2.5, 1.0, 1.0]), Ok(2));
        editor.selected_points = BTreeSet::from([0]);
        editor.remove_trail_points(0, &BTreeSet::from([0])).unwrap();
        assert!(editor.selected_points.is_empty());

        let expected = vec![
            [2.0, 5.0, 1.0],
            [2.5, 1.0, 1.0],
            [3.0, 1.0, 1.0],
            [4.0, 1.0, 1.0],
        ];
        assert_eq!(editor.pack.trails[0].points, expected);
        assert_eq!(Editor::open(&dir).pack.trails[0].points, expected, "saved");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recorder_pauses_itself() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        let start = [10.0, 0.0, 0.0];
        editor.start_trail(50, start).unwrap();
        let pos = walk(&mut editor, 50, start, 3);

        // Teleport.
        let far = [pos[0] + 300.0, pos[1], pos[2]];
        let message = editor.trail_tick(Some((50, far))).unwrap();
        assert!(message.contains("300 m"), "{message}");
        assert_eq!(editor.recording(), Some((0, true)));
        assert_eq!(
            editor.pack.trails[0].point_count(),
            4,
            "the far point isn't recorded"
        );

        // Map change.
        editor.resume_trail(50, far).unwrap();
        let message = editor.trail_tick(Some((51, far))).unwrap();
        assert!(message.contains("map 50"), "{message}");
        assert!(editor.resume_trail(51, far).is_err(), "other map");
        assert_eq!(editor.recording(), Some((0, true)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn short_trails_are_discarded_and_files_deleted() {
        let dir = temp_dir();
        let mut editor = Editor::open(&dir);
        editor.start_trail(50, [1.0; 3]).unwrap();
        let file = dir.join("trails").join("map50_1.trl");
        assert!(file.exists(), "written when started");
        editor.stop_trail();
        assert!(editor.pack.trails.is_empty());
        assert!(!file.exists());

        editor.start_trail(50, [1.0; 3]).unwrap();
        walk(&mut editor, 50, [1.0; 3], 3);
        editor.remove_trail(0);
        assert_eq!(editor.pack.trails.len(), 1, "can't delete while recording");
        editor.stop_trail();
        assert!(file.exists());
        editor.selection = Some(Selection::Trail(0));
        editor.remove_trail(0);
        assert_eq!(editor.selection, None);
        assert!(editor.pack.trails.is_empty());
        assert!(!file.exists());
        assert!(Editor::open(&dir).pack.trails.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_trail_file_keeps_the_trail() {
        let dir = temp_dir();
        fs::write(
            dir.join(CAPTURES_FILE),
            r#"<OverlayData><POIs><Trail type="a" trailData="data/gone.trl" GUID="x"/></POIs></OverlayData>"#,
        )
        .unwrap();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.pack.trails.len(), 1);
        assert!(editor.pack.trails[0].points.is_empty());
        assert!(editor.pack.category("a").is_some(), "category created");
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Warning);

        // Saving keeps it, and deleting it doesn't touch files the editor didn't create.
        editor.capture(50, [1.0; 3]).unwrap();
        assert_eq!(Editor::open(&dir).pack.trails.len(), 1);
        editor.remove_trail(0);
        assert_eq!(editor.status.as_ref().unwrap().kind, StatusKind::Info);
        fs::remove_dir_all(&dir).unwrap();
    }
}
