//! The declarative filter attributes (docs/02 §5): conditions an overlay checks to show or hide
//! a marker or trail, with no script. They go on markers, trails and categories, and are
//! inherited from the categories like every attribute.
//!
//! The editor only checks that the values are ones the overlays understand. It doesn't evaluate
//! them (that needs the GW2 API or the game's state).

/// The kind of value an attribute takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A `,`-separated list of names. Each entry is the name written, and the aliases the
    /// overlays also accept.
    Names(&'static [&'static [&'static str]]),
    /// A `,`-separated list of whole numbers.
    Ints,
    /// One whole number. `int16` when TacO reads it as one.
    Int { int16: bool },
    /// A `,`-separated list of free-form ids (raid encounters).
    Ids,
    /// A UTC cron expression (Cronos syntax).
    Cron,
    /// Minutes, more than 0.
    Minutes,
}

/// One filter attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    /// The attribute, in its canonical casing.
    pub attr: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: Kind,
    /// Which overlays read it: TacO, Blish HUD Pathing, TaimiHUD.
    pub support: [bool; 3],
}

pub const FESTIVALS: &[&[&str]] = &[
    &["halloween"],
    &["wintersday"],
    &["superadventurefestival"],
    &["lunarnewyear"],
    &["festivalofthefourwinds"],
    &["dragonbash"],
];

pub const MOUNTS: &[&[&str]] = &[
    &["jackal"],
    &["griffon"],
    &["springer"],
    &["skimmer"],
    &["raptor"],
    &["rollerbeetle"],
    &["warclaw"],
    &["skyscale"],
    &["skiff"],
    &["siegeturtle"],
];

pub const PROFESSIONS: &[&[&str]] = &[
    &["guardian"],
    &["warrior"],
    &["engineer"],
    &["ranger"],
    &["thief"],
    &["elementalist"],
    &["mesmer"],
    &["necromancer"],
    &["revenant"],
];

pub const RACES: &[&[&str]] = &[&["asura"], &["charr"], &["human"], &["norn"], &["sylvari"]];

pub const MAP_TYPES: &[&[&str]] = &[
    &["unknown"],
    &["redirect"],
    &["charactercreate"],
    &["pvp"],
    &["gvg"],
    &["instance"],
    &["public"],
    &["tournament"],
    &["tutorial"],
    &["usertournament"],
    &["eternalbattlegrounds", "center"],
    &["blueborderlands", "bluehome"],
    &["greenborderlands", "greenhome"],
    &["redborderlands", "redhome"],
    &["fortunesvale"],
    &["obsidiansanctum", "jumppuzzle"],
    &["edgeofthemists"],
    &["publicmini"],
    &["wvwlounge"],
];

pub const CONDITIONS: &[Condition] = &[
    Condition {
        attr: "festival",
        label: "Festivals",
        hint: "Shown only while one of them is on. TacO can't tell (its players switch it by hand).",
        kind: Kind::Names(FESTIVALS),
        support: [true, true, true],
    },
    Condition {
        attr: "mount",
        label: "Mounts",
        hint: "Shown only while riding one of them.",
        kind: Kind::Names(MOUNTS),
        support: [false, true, true],
    },
    Condition {
        attr: "profession",
        label: "Professions",
        hint: "Shown only to characters of one of them.",
        kind: Kind::Names(PROFESSIONS),
        support: [false, true, true],
    },
    Condition {
        attr: "race",
        label: "Races",
        hint: "Shown only to characters of one of them.",
        kind: Kind::Names(RACES),
        support: [false, true, true],
    },
    Condition {
        attr: "specialization",
        label: "Specializations",
        hint: "Specialization ids (e.g. 27 Dragonhunter, 56 Weaver), from /v2/specializations.\n\
               Shown only when one of them is in the third trait slot (that's all MumbleLink tells).",
        kind: Kind::Ints,
        support: [false, true, true],
    },
    Condition {
        attr: "maptype",
        label: "Map types",
        hint: "Shown only on maps of one of these types (MumbleLink's map type).",
        kind: Kind::Names(MAP_TYPES),
        support: [false, true, true],
    },
    Condition {
        attr: "achievementId",
        label: "Achievement",
        hint: "Achievement id (from /v2/achievements). Hidden once it's done.\n\
               The overlays need the player's API key for this.",
        kind: Kind::Int { int16: true },
        support: [true, true, true],
    },
    Condition {
        attr: "achievementBit",
        label: "Achievement bit",
        hint: "With an achievement: hidden once this part of it is done. 0-based index into the API's\n\
               `bits` (not the order the wiki or the game shows).",
        kind: Kind::Int { int16: false },
        support: [true, true, true],
    },
    Condition {
        attr: "schedule",
        label: "Schedule",
        hint: "When it shows, as a UTC cron expression: minute hour day month weekday\n\
               (or with seconds first). E.g. \"20 2/6 * * *\". Needs a duration.",
        kind: Kind::Cron,
        support: [false, true, true],
    },
    Condition {
        attr: "schedule-duration",
        label: "Schedule duration",
        hint: "How long it shows from each start of the schedule, in minutes.",
        kind: Kind::Minutes,
        support: [false, true, true],
    },
    Condition {
        attr: "raid",
        label: "Raid encounters",
        hint: "Raid encounter ids from /v2/raids (e.g. vale_guardian, xera). Hidden once all are\n\
               cleared this week.",
        kind: Kind::Ids,
        support: [false, true, true],
    },
];

#[cfg(test)]
pub fn condition(attr: &str) -> Option<&'static Condition> {
    CONDITIONS
        .iter()
        .find(|c| c.attr.eq_ignore_ascii_case(attr))
}

/// "TacO, TaimiHUD"-style list of the overlays that read it, or `None` if all three do.
pub fn support_note(support: [bool; 3]) -> Option<String> {
    if support == [true; 3] {
        return None;
    }
    let names: Vec<&str> = ["TacO", "Blish HUD", "TaimiHUD"]
        .into_iter()
        .zip(support)
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
    Some(format!("Only {}.", names.join(" and ")))
}

/// The entries of a list value, trimmed, empty ones dropped.
pub fn entries(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|e| !e.is_empty())
}

/// Whether list entry `entry` (any casing, any alias) is `names[index]`.
pub fn is_name(names: &[&[&str]], index: usize, entry: &str) -> bool {
    names
        .get(index)
        .is_some_and(|aliases| aliases.iter().any(|a| a.eq_ignore_ascii_case(entry)))
}

/// `value` with `names[index]` added or removed, in the order of `names` (unknown entries are
/// kept at the end). Empty when nothing is left.
pub fn toggle_name(names: &[&[&str]], value: &str, index: usize, on: bool) -> String {
    let mut picked: Vec<usize> = Vec::new();
    let mut unknown: Vec<&str> = Vec::new();
    for entry in entries(value) {
        match (0..names.len()).find(|&i| is_name(names, i, entry)) {
            Some(i) if !picked.contains(&i) => picked.push(i),
            Some(_) => {}
            None => unknown.push(entry),
        }
    }
    picked.retain(|&i| i != index);
    if on {
        picked.push(index);
    }
    picked.sort_unstable();
    picked
        .into_iter()
        .map(|i| names[i][0])
        .chain(unknown)
        .collect::<Vec<_>>()
        .join(",")
}

/// What's wrong with `value` for `condition`, for the author to fix. Empty if nothing.
pub fn check(condition: &Condition, value: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if value.trim().is_empty() {
        problems.push("Empty: the overlays ignore it. Clear it instead.".to_owned());
        return problems;
    }
    if matches!(condition.kind, Kind::Names(_) | Kind::Ints | Kind::Ids)
        && value.split(',').any(|e| e != e.trim())
    {
        problems.push("Write the list without spaces (a,b,c): TacO doesn't trim them.".to_owned());
    }
    match condition.kind {
        Kind::Names(names) => {
            let unknown: Vec<&str> = entries(value)
                .filter(|e| !(0..names.len()).any(|i| is_name(names, i, e)))
                .collect();
            if !unknown.is_empty() {
                problems.push(format!(
                    "Not known to the overlays: {}.",
                    unknown.join(", ")
                ));
            }
        }
        Kind::Ints => {
            let bad: Vec<&str> = entries(value)
                .filter(|e| e.parse::<u32>().is_err())
                .collect();
            if !bad.is_empty() {
                problems.push(format!("Not whole numbers: {}.", bad.join(", ")));
            }
        }
        Kind::Int { int16 } => match value.trim().parse::<i64>() {
            Err(_) => problems.push("Not a whole number.".to_owned()),
            Ok(n) if n < 0 => problems.push("Can't be negative.".to_owned()),
            Ok(n) if int16 && n > i64::from(i16::MAX) => problems.push(format!(
                "TacO reads it as a 16-bit number: above {} it gets a different one.",
                i16::MAX
            )),
            Ok(_) => {}
        },
        Kind::Ids => {
            let bad: Vec<&str> = entries(value)
                .filter(|e| !e.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .collect();
            if !bad.is_empty() {
                problems.push(format!(
                    "Ids are lowercase words joined by _ (e.g. vale_guardian): {}.",
                    bad.join(", ")
                ));
            }
        }
        Kind::Cron => problems.extend(check_cron(value)),
        Kind::Minutes => match value.trim().parse::<f32>() {
            Ok(m) if m > 0.0 && m.is_finite() => {}
            _ => problems.push("Must be a number of minutes above 0.".to_owned()),
        },
    }
    problems
}

/// A rough check of a Cronos expression: 5 fields (or 6, seconds first), each made of the
/// characters Cronos accepts. Names (`MON`, `JAN`) are allowed.
fn check_cron(value: &str) -> Option<String> {
    let fields: Vec<&str> = value.split_whitespace().collect();
    if !(5..=6).contains(&fields.len()) {
        return Some(format!(
            "Needs 5 fields (minute hour day month weekday) or 6 (seconds first), not {}.",
            fields.len()
        ));
    }
    let bad = fields.iter().find(|f| {
        !f.chars()
            .all(|c| c.is_ascii_alphanumeric() || "*,-/?#".contains(c))
    })?;
    Some(format!("\"{bad}\" isn't a cron field."))
}

/// Problems that come from two attributes together: `own` with what the item inherits
/// (`get` gives the effective value of an attribute).
pub fn check_together<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Vec<String> {
    let mut problems = Vec::new();
    match (get("schedule"), get("schedule-duration")) {
        (Some(_), None) => problems
            .push("A schedule needs a duration, or the overlays ignore the schedule.".to_owned()),
        (None, Some(_)) => {
            problems.push("A schedule duration does nothing without a schedule.".to_owned())
        }
        _ => {}
    }
    if get("achievementBit").is_some() && get("achievementId").is_none() {
        problems.push("An achievement bit does nothing without an achievement.".to_owned());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cond(attr: &str) -> &'static Condition {
        condition(attr).unwrap()
    }

    #[test]
    fn lists() {
        assert!(check(cond("mount"), "griffon,skyscale").is_empty());
        assert!(check(cond("Mount"), "Griffon").is_empty(), "any casing");
        assert_eq!(
            check(cond("mount"), "griffon, bunny"),
            vec![
                "Write the list without spaces (a,b,c): TacO doesn't trim them.".to_owned(),
                "Not known to the overlays: bunny.".to_owned()
            ]
        );
        assert!(
            check(cond("maptype"), "center,redhome").is_empty(),
            "aliases"
        );
        assert_eq!(check(cond("specialization"), "27,x").len(), 1);
        assert!(check(cond("raid"), "vale_guardian,xera").is_empty());
        assert_eq!(check(cond("raid"), "Vale Guardian").len(), 1);
    }

    #[test]
    fn numbers_and_schedules() {
        assert!(check(cond("achievementId"), "1234").is_empty());
        assert_eq!(check(cond("achievementId"), "40000").len(), 1, "int16");
        assert!(check(cond("achievementBit"), "40000").is_empty());
        assert_eq!(check(cond("achievementBit"), "-1").len(), 1);
        assert!(check(cond("schedule"), "20 2/6 * * *").is_empty());
        assert!(check(cond("schedule"), "0 0 * * WED").is_empty());
        assert!(
            check(cond("schedule"), "0 0 0 * * 3").is_empty(),
            "with seconds"
        );
        assert_eq!(check(cond("schedule"), "0 0 *").len(), 1);
        assert_eq!(check(cond("schedule"), "0 0 * * ;").len(), 1);
        assert!(check(cond("schedule-duration"), "15").is_empty());
        assert_eq!(check(cond("schedule-duration"), "0").len(), 1);
        assert_eq!(check(cond("festival"), " ").len(), 1, "empty");
    }

    #[test]
    fn pairs() {
        let get = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
        };
        assert!(
            check_together(get(&[
                ("schedule", "0 0 * * *"),
                ("schedule-duration", "60")
            ]))
            .is_empty()
        );
        assert_eq!(check_together(get(&[("schedule", "0 0 * * *")])).len(), 1);
        assert_eq!(check_together(get(&[("achievementBit", "2")])).len(), 1);
        assert!(check_together(get(&[("achievementId", "5"), ("achievementBit", "2")])).is_empty());
    }

    #[test]
    fn toggling() {
        assert_eq!(toggle_name(MOUNTS, "", 1, true), "griffon");
        assert_eq!(
            toggle_name(MOUNTS, "skyscale", 1, true),
            "griffon,skyscale",
            "ordered"
        );
        assert_eq!(
            toggle_name(MOUNTS, "Griffon,bunny", 1, false),
            "bunny",
            "unknown kept"
        );
        assert_eq!(toggle_name(MAP_TYPES, "center", 10, false), "");
        assert_eq!(
            toggle_name(MAP_TYPES, "center", 10, true),
            "eternalbattlegrounds"
        );
        assert!(is_name(MAP_TYPES, 10, "CENTER"));
    }

    #[test]
    fn support() {
        assert_eq!(support_note([true; 3]), None);
        assert_eq!(
            support_note([false, true, true]).as_deref(),
            Some("Only Blish HUD and TaimiHUD.")
        );
    }
}
