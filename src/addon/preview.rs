//! Draws the pack's markers and trails in the world, on the minimap and on the world map,
//! straight from the editor's data, so every edit shows at once.
//!
//! Drawn on ImGui's background layer: over the game, under every window, without depth (no
//! occlusion, like TacO). The look follows docs/05 §2–4: icons sized 2 m × `iconSize` and
//! clamped to `minSize`/`maxSize`, `heightOffset`, `alpha`, `fadeNear`/`fadeFar`, flat trail
//! ribbons 2 × 0.508 m × `trailScale` wide; on the maps `mapDisplaySize`,
//! `scaleOnMapWithZoom`, `mapFadeoutScaleLevel`; and the three `*Visibility` flags. Attributes
//! are inherited from the categories.

use super::{Live, thumbnails};
use crate::{
    editor::{Editor, Selection},
    geometry::{add, cross, distance, lerp, normalize, scale, sub},
    pack::Pack,
    trail::Trail,
    view::{self, Camera, MapView, MarkerStyle, TrailStyle, clip_near, fade_factor},
};
use nexus::{
    imgui::{DrawListMut, TextureId, Ui},
    texture::get_texture,
};
use std::{collections::HashMap, path::Path};

/// Selected markers and trails.
const HIGHLIGHT: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
/// Selected trail points: blue, against the yellow of the selected trail.
const POINT_SELECTED: [f32; 4] = [0.25, 0.5, 1.0, 1.0];
/// Trails without a texture: TacO's colour for the trail being recorded, see-through.
const TRAIL_COLOR: [f32; 4] = [0.2, 0.7, 1.0, 0.6];
/// Trails without fade distances are drawn up to this far (TacO clips its trails at 150 m).
const TRAIL_MAX_DISTANCE_M: f32 = 300.0;
/// The selected trail's points are marked up to this far.
const POINTS_MAX_DISTANCE_M: f32 = 150.0;
/// Narrowest trail on the maps, in pixels.
const MAP_TRAIL_MIN_WIDTH: f32 = 4.0;

pub(super) fn draw(ui: &Ui, editor: &Editor, live: &Live, default_icon: &str) {
    if !editor.settings.preview || !live.in_game {
        return;
    }
    let screen = ui.io().display_size;
    let draw_list = ui.get_background_draw_list();
    let mut ctx = Context {
        editor,
        live,
        textures: Textures {
            ui,
            dir: editor.dir(),
            loaded: HashMap::new(),
        },
        default_icon: get_texture(default_icon).map(|t| t.id()),
    };
    let map = MapView::new(&live.map, screen);

    if !live.map_open
        && let Some(camera) = Camera::new(live.camera, live.camera_front, live.fov, screen)
    {
        let prims = ctx.world(&camera);
        // Like TacO, keep the world off the minimap: draw it around the minimap's rectangle.
        let regions = match &map {
            Some(view) => around(screen, view.rect),
            None => vec![[[0.0, 0.0], screen]],
        };
        for [min, max] in regions {
            draw_list.with_clip_rect(min, max, || {
                for prim in prims.iter().filter(|p| p.overlaps(min, max)) {
                    prim.emit(&draw_list);
                }
            });
        }
    }

    if let Some(view) = map {
        let prims = ctx.map(&view);
        let [min, max] = view.rect;
        draw_list.with_clip_rect(min, max, || {
            for prim in &prims {
                prim.emit(&draw_list);
            }
        });
    }
}

/// Up to three rectangles covering the screen except `hole`.
fn around(screen: [f32; 2], hole: [[f32; 2]; 2]) -> Vec<[[f32; 2]; 2]> {
    let [w, h] = screen;
    let [[x1, y1], [x2, y2]] = hole;
    [
        [[0.0, 0.0], [x1, h]],
        [[x1, 0.0], [w, y1]],
        [[x1, y2], [w, h]],
        [[x2, y1], [w, y2]],
    ]
    .into_iter()
    .filter(|[min, max]| max[0] > min[0] && max[1] > min[1])
    .collect()
}

/// Something to draw, kept so it can be emitted under several clip rectangles.
enum Prim {
    Image {
        texture: TextureId,
        min: [f32; 2],
        max: [f32; 2],
        color: [f32; 4],
    },
    Quad {
        texture: Option<TextureId>,
        corners: [[f32; 2]; 4],
        /// Texture `v` at the first two and last two corners.
        v: [f32; 2],
        color: [f32; 4],
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        color: [f32; 4],
        filled: bool,
    },
    Rect {
        min: [f32; 2],
        max: [f32; 2],
        color: [f32; 4],
    },
    Text {
        at: [f32; 2],
        color: [f32; 4],
        text: String,
    },
}

impl Prim {
    fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        match self {
            Self::Image { min, max, .. } | Self::Rect { min, max, .. } => (*min, *max),
            Self::Quad { corners, .. } => bounds(corners),
            Self::Circle { center, radius, .. } => (
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ),
            // Roughly: a line of ImGui's default font.
            Self::Text { at, text, .. } => (*at, [at[0] + 8.0 * text.len() as f32, at[1] + 16.0]),
        }
    }

    fn overlaps(&self, min: [f32; 2], max: [f32; 2]) -> bool {
        let (a, b) = self.bounds();
        b[0] >= min[0] && b[1] >= min[1] && a[0] <= max[0] && a[1] <= max[1]
    }

    fn emit(&self, draw_list: &DrawListMut) {
        match self {
            Self::Image {
                texture,
                min,
                max,
                color,
            } => draw_list
                .add_image(*texture, *min, *max)
                .col(*color)
                .build(),
            Self::Quad {
                texture,
                corners: [p1, p2, p3, p4],
                v: [v0, v1],
                color,
            } => match texture {
                Some(texture) => draw_list
                    .add_image_quad(*texture, *p1, *p2, *p3, *p4)
                    // `v` counts forward along the trail, but the image's top must face
                    // forward, as in TacO, Blish HUD and TaimiHUD (arrows point the way the
                    // trail goes), so it runs the other way. The sampler repeats, so negative
                    // values tile like positive ones.
                    .uv([0.0, -*v0], [1.0, -*v0], [1.0, -*v1], [0.0, -*v1])
                    .col(*color)
                    .build(),
                None => {
                    draw_list
                        .add_triangle(*p1, *p2, *p3, *color)
                        .filled(true)
                        .build();
                    draw_list
                        .add_triangle(*p1, *p3, *p4, *color)
                        .filled(true)
                        .build();
                }
            },
            Self::Circle {
                center,
                radius,
                color,
                filled,
            } => draw_list
                .add_circle(*center, *radius, *color)
                .filled(*filled)
                .thickness(2.0)
                .build(),
            Self::Rect { min, max, color } => draw_list
                .add_rect(*min, *max, *color)
                .thickness(2.0)
                .build(),
            Self::Text { at, color, text } => draw_list.add_text(*at, *color, text),
        }
    }
}

fn bounds(points: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for [x, y] in points {
        min = [min[0].min(*x), min[1].min(*y)];
        max = [max[0].max(*x), max[1].max(*y)];
    }
    (min, max)
}

struct Context<'a, 'ui> {
    editor: &'a Editor,
    live: &'a Live,
    textures: Textures<'a, 'ui>,
    /// The addon's icon, for markers without one.
    default_icon: Option<TextureId>,
}

impl Context<'_, '_> {
    fn pack(&self) -> &Pack {
        &self.editor.pack
    }

    fn poi_selected(&self, index: usize) -> bool {
        matches!(&self.editor.selection, Some(Selection::Pois(set)) if set.contains(&index))
    }

    fn trail_highlighted(&self, index: usize) -> bool {
        self.editor.selection == Some(Selection::Trail(index))
            || self.editor.recording().map(|(i, _)| i) == Some(index)
    }

    fn icon(&mut self, path: Option<&str>) -> Option<TextureId> {
        path.and_then(|p| self.textures.get(p))
            .or(self.default_icon)
    }

    // ---- The world ----

    fn world(&mut self, camera: &Camera) -> Vec<Prim> {
        let mut prims = Vec::new();
        let editor = self.editor;
        let map_id = self.live.map_id;

        for (i, trail) in editor.pack.trails.iter().enumerate() {
            if trail.map_id == map_id && editor.settings.is_shown(&trail.category) {
                self.world_trail(&mut prims, camera, trail, self.trail_highlighted(i));
            }
        }
        if let Some(Selection::Trail(i)) = editor.selection
            && let Some(trail) = editor.pack.trails.get(i)
            && trail.map_id == map_id
            && editor.settings.is_shown(&trail.category)
        {
            world_points(&mut prims, camera, trail, &editor.selected_points);
        }

        let mut markers = Vec::new();
        for (i, poi) in editor.pack.pois.iter().enumerate() {
            if poi.map_id != map_id || !editor.settings.is_shown(&poi.category) {
                continue;
            }
            let style = MarkerStyle::of(self.pack(), poi);
            if !style.visible.world {
                continue;
            }
            let [x, y, z] = poi.pos;
            let anchor = [x, y + style.height_offset, z];
            let in_view = camera.to_view(anchor);
            let Some(center) = camera.project_view(in_view) else {
                continue;
            };
            let Some(fade) = fade_factor(style.fade, distance(camera.eye(), anchor)) else {
                continue;
            };
            let half = (style.icon_size * camera.pixels_per_metre(in_view[2]))
                .max(style.min_size)
                .min(style.max_size);
            if camera.on_screen(center, half) {
                markers.push((in_view[2], i, center, half, style.alpha * fade, style.icon));
            }
        }
        // Far to near, so nearer icons are drawn on top.
        markers.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, i, center, half, alpha, icon) in markers {
            let texture = self.icon(icon.as_deref());
            icon_prims(&mut prims, texture, center, half, alpha);
            if self.poi_selected(i) {
                selection_prims(&mut prims, center, half, i);
            }
        }
        prims
    }

    /// A flat ribbon, in pieces short enough near the camera (see `view::trail_pieces`).
    fn world_trail(
        &mut self,
        prims: &mut Vec<Prim>,
        camera: &Camera,
        trail: &Trail,
        highlight: bool,
    ) {
        let style = TrailStyle::of(self.pack(), trail);
        if !style.visible.world {
            return;
        }
        let texture = style.texture.as_deref().and_then(|t| self.textures.get(t));
        let width = 2.0 * style.half_width;
        let max_distance = style.fade.map_or(TRAIL_MAX_DISTANCE_M, |(_, far)| far);

        for section in trail.sections() {
            // Metres along the section, for the texture's v (one repeat per trail width).
            let mut travelled = 0.0;
            for segment in section.windows(2) {
                let (a, b) = (segment[0], segment[1]);
                let length = distance(a, b);
                let (v0, v1) = (travelled / width, (travelled + length) / width);
                travelled += length;

                let middle = lerp(a, b, 0.5);
                let from_camera = distance(camera.eye(), middle);
                if length < 1e-4 || from_camera - length > max_distance {
                    continue;
                }
                let Some(fade) = fade_factor(style.fade, from_camera) else {
                    continue;
                };
                // Horizontal, across the segment. Straight up: any horizontal direction.
                let side = normalize(cross([b[0] - a[0], 0.0, b[2] - a[2]], [0.0, 1.0, 0.0]))
                    .unwrap_or([1.0, 0.0, 0.0]);
                let side = scale(side, style.half_width);

                // Keep the part in front of the camera.
                let Some((t0, t1)) = clip_near(camera.to_view(a)[2], camera.to_view(b)[2]) else {
                    continue;
                };
                let (a, b) = (lerp(a, b, t0), lerp(a, b, t1));
                let (v0, v1) = (v0 + (v1 - v0) * t0, v0 + (v1 - v0) * t1);

                // ImGui maps textures linearly on screen, without perspective: short pieces
                // near the camera keep the texture from warping and sliding as it moves.
                let nearest = camera.to_view(a)[2].min(camera.to_view(b)[2]);
                let pieces = view::trail_pieces(distance(a, b), nearest);
                let color = trail_color(texture.is_some(), highlight, style.alpha * fade);
                for k in 0..pieces {
                    let (s0, s1) = (k as f32 / pieces as f32, (k + 1) as f32 / pieces as f32);
                    let (pa, pb) = (lerp(a, b, s0), lerp(a, b, s1));
                    let corners = [sub(pa, side), add(pa, side), add(pb, side), sub(pb, side)];
                    let Some(corners) = project_all(camera, corners) else {
                        continue;
                    };
                    let (min, max) = bounds(&corners);
                    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
                    if !camera.on_screen(center, (max[0] - min[0]).max(max[1] - min[1]) / 2.0) {
                        continue;
                    }
                    prims.push(Prim::Quad {
                        texture,
                        corners,
                        v: [v0 + (v1 - v0) * s0, v0 + (v1 - v0) * s1],
                        color,
                    });
                }
            }
        }
    }

    // ---- The minimap or the world map ----

    fn map(&mut self, view: &MapView) -> Vec<Prim> {
        let mut prims = Vec::new();
        let editor = self.editor;
        let map_id = self.live.map_id;
        let world_map = self.live.map_open;
        let shown = |visible: view::Visibility| {
            if world_map {
                visible.map
            } else {
                visible.minimap
            }
        };

        for (i, trail) in editor.pack.trails.iter().enumerate() {
            if trail.map_id != map_id || !editor.settings.is_shown(&trail.category) {
                continue;
            }
            let style = TrailStyle::of(self.pack(), trail);
            if !shown(style.visible) {
                continue;
            }
            let texture = style.texture.as_deref().and_then(|t| self.textures.get(t));
            // TacO: half the marker size on the map. At least wide enough to show the texture.
            let (size, fade) = style.map.on(view);
            let width = (size * 0.5).max(MAP_TRAIL_MIN_WIDTH);
            let color = trail_color(
                texture.is_some(),
                self.trail_highlighted(i),
                style.alpha * fade,
            );
            for section in trail.sections() {
                let points: Vec<[f32; 2]> = section.iter().map(|&p| view.project(p)).collect();
                map_ribbon(&mut prims, &points, width, texture, color, view.rect);
            }
        }

        for (i, poi) in editor.pack.pois.iter().enumerate() {
            if poi.map_id != map_id || !editor.settings.is_shown(&poi.category) {
                continue;
            }
            let style = MarkerStyle::of(self.pack(), poi);
            if !shown(style.visible) {
                continue;
            }
            let center = view.project(poi.pos);
            if !view.contains(center) {
                continue;
            }
            let (size, fade) = style.map.on(view);
            let alpha = style.alpha * fade;
            if size < 1.0 || alpha <= 0.0 {
                continue;
            }
            let texture = self.icon(style.icon.as_deref());
            icon_prims(&mut prims, texture, center, size / 2.0, alpha);
            if self.poi_selected(i) {
                selection_prims(&mut prims, center, size / 2.0, i);
            }
        }
        prims
    }
}

/// A trail's tint: its texture as is, or a see-through blue without one; yellow when selected.
fn trail_color(textured: bool, highlight: bool, alpha: f32) -> [f32; 4] {
    let [r, g, b, a] = match (textured, highlight) {
        (true, false) => [1.0; 4],
        (true, true) => HIGHLIGHT,
        (false, false) => TRAIL_COLOR,
        (false, true) => [HIGHLIGHT[0], HIGHLIGHT[1], HIGHLIGHT[2], TRAIL_COLOR[3]],
    };
    [r, g, b, a * alpha]
}

/// A trail on a map: a flat ribbon along `points` (screen pixels), its texture repeating once
/// per `width` along the way, as TaimiHUD does on its maps. Measured on screen, the texture
/// keeps its shape whether the map pans, rotates or zooms.
fn map_ribbon(
    prims: &mut Vec<Prim>,
    points: &[[f32; 2]],
    width: f32,
    texture: Option<TextureId>,
    color: [f32; 4],
    [rect_min, rect_max]: [[f32; 2]; 2],
) {
    let Some(&first) = points.first() else {
        return;
    };
    let half = width / 2.0;
    // Pixels along the section, for the texture's v.
    let mut travelled = 0.0;
    let mut start = first;
    for (k, &end) in points.iter().enumerate().skip(1) {
        let d = [end[0] - start[0], end[1] - start[1]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        // Zoomed out, many points fall on the same pixel: join them into one piece.
        if length < 1.0 && k + 1 < points.len() {
            continue;
        }
        if length < 1e-3 {
            start = end;
            continue;
        }
        let side = [-d[1] / length * half, d[0] / length * half];
        let v = [travelled / width, (travelled + length) / width];
        travelled += length;
        let corners = [
            [start[0] - side[0], start[1] - side[1]],
            [start[0] + side[0], start[1] + side[1]],
            [end[0] + side[0], end[1] + side[1]],
            [end[0] - side[0], end[1] - side[1]],
        ];
        let (min, max) = bounds(&corners);
        if max[0] >= rect_min[0]
            && max[1] >= rect_min[1]
            && min[0] <= rect_max[0]
            && min[1] <= rect_max[1]
        {
            prims.push(Prim::Quad {
                texture,
                corners,
                v,
                color,
            });
        }
        start = end;
    }
}

fn icon_prims(
    prims: &mut Vec<Prim>,
    texture: Option<TextureId>,
    center: [f32; 2],
    half: f32,
    alpha: f32,
) {
    let [cx, cy] = center;
    prims.push(match texture {
        Some(texture) => Prim::Image {
            texture,
            min: [cx - half, cy - half],
            max: [cx + half, cy + half],
            color: [1.0, 1.0, 1.0, alpha],
        },
        None => Prim::Circle {
            center,
            radius: half,
            color: [1.0, 0.6, 0.2, alpha],
            filled: true,
        },
    });
}

/// An outline and the marker's number.
fn selection_prims(prims: &mut Vec<Prim>, [cx, cy]: [f32; 2], half: f32, index: usize) {
    let h = half + 2.0;
    prims.push(Prim::Rect {
        min: [cx - h, cy - h],
        max: [cx + h, cy + h],
        color: HIGHLIGHT,
    });
    prims.push(Prim::Text {
        at: [cx - h, cy + h + 2.0],
        color: HIGHLIGHT,
        text: format!("#{}", index + 1),
    });
}

/// Dots on the points of the selected trail; the selected points in blue, numbered.
fn world_points(
    prims: &mut Vec<Prim>,
    camera: &Camera,
    trail: &Trail,
    selected: &std::collections::BTreeSet<usize>,
) {
    for (number, &(index, _)) in trail.numbered_points().iter().enumerate() {
        let point = trail.points[index];
        let in_view = camera.to_view(point);
        if in_view[2] > POINTS_MAX_DISTANCE_M {
            continue;
        }
        let Some(center) = camera.project_view(in_view) else {
            continue;
        };
        if !camera.on_screen(center, 8.0) {
            continue;
        }
        if selected.contains(&index) {
            prims.push(Prim::Circle {
                center,
                radius: 6.0,
                color: POINT_SELECTED,
                filled: true,
            });
            prims.push(Prim::Circle {
                center,
                radius: 7.0,
                color: [1.0, 1.0, 1.0, 1.0],
                filled: false,
            });
            prims.push(Prim::Text {
                at: [center[0] + 9.0, center[1] - 7.0],
                color: POINT_SELECTED,
                text: format!("point {}", number + 1),
            });
        } else {
            prims.push(Prim::Circle {
                center,
                radius: 3.0,
                color: [1.0, 1.0, 1.0, 0.9],
                filled: true,
            });
        }
    }
}

/// Textures by path, looked up once per frame.
struct Textures<'a, 'ui> {
    ui: &'a Ui<'ui>,
    dir: &'a Path,
    loaded: HashMap<String, Option<TextureId>>,
}

impl Textures<'_, '_> {
    fn get(&mut self, path: &str) -> Option<TextureId> {
        if let Some(id) = self.loaded.get(path) {
            return *id;
        }
        let id = thumbnails::texture(self.ui, self.dir, path).map(|t| t.id());
        self.loaded.insert(path.to_owned(), id);
        id
    }
}

fn project_all(camera: &Camera, corners: [[f32; 3]; 4]) -> Option<[[f32; 2]; 4]> {
    let mut out = [[0.0; 2]; 4];
    for (o, corner) in out.iter_mut().zip(corners) {
        *o = camera.project(corner)?;
    }
    Some(out)
}
