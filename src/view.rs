//! The in-game preview: camera projection (docs/05 §1) and how markers and trails look, from
//! their attributes and their categories' (docs/05 §2–3, docs/02).

use crate::{
    geometry::{cross, dot, normalize, sub},
    pack::{Pack, Poi},
    trail::Trail,
};

/// Closest distance to the camera that is drawn, in metres.
pub const NEAR: f32 = 0.1;

/// Game units (inches) per metre: `fadeNear` / `fadeFar` are in game units (docs/01 §6.1).
const INCHES_PER_METRE: f32 = 39.37;

/// Half the width of a trail ribbon at `trailScale` 1: 20 game units (docs/05 §3.1).
pub const TRAIL_HALF_WIDTH_M: f32 = 0.508;

/// The game camera, from MumbleLink.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    eye: [f32; 3],
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    /// tan(vertical FOV / 2).
    tan_half_fov: f32,
    /// Viewport in pixels.
    size: [f32; 2],
}

impl Camera {
    /// `front` is MumbleLink's camera front, `fov` the vertical field of view in radians (from
    /// the identity), `size` the viewport in pixels. The world is left-handed with Y up, and the
    /// camera's up is always +Y (docs/05 §1). `None` if the values can't make a camera.
    pub fn new(eye: [f32; 3], front: [f32; 3], fov: f32, size: [f32; 2]) -> Option<Self> {
        let forward = normalize(front)?;
        let right = normalize(cross([0.0, 1.0, 0.0], forward))?;
        let up = cross(forward, right);
        let usable = fov.is_finite() && fov > 0.01 && fov < 3.1 && size[0] >= 1.0 && size[1] >= 1.0;
        usable.then(|| Self {
            eye,
            forward,
            right,
            up,
            tan_half_fov: (fov / 2.0).tan(),
            size,
        })
    }

    pub fn eye(&self) -> [f32; 3] {
        self.eye
    }

    /// World → camera space: x right, y up, z forward (metres).
    pub fn to_view(&self, p: [f32; 3]) -> [f32; 3] {
        let d = sub(p, self.eye);
        [dot(d, self.right), dot(d, self.up), dot(d, self.forward)]
    }

    /// Camera space → screen pixels (origin top left), or `None` behind the near plane.
    pub fn project_view(&self, v: [f32; 3]) -> Option<[f32; 2]> {
        if v[2] < NEAR {
            return None;
        }
        let [w, h] = self.size;
        let ndc_y = v[1] / (v[2] * self.tan_half_fov);
        let ndc_x = v[0] / (v[2] * self.tan_half_fov * (w / h));
        Some([(ndc_x + 1.0) * 0.5 * w, (1.0 - ndc_y) * 0.5 * h])
    }

    pub fn project(&self, p: [f32; 3]) -> Option<[f32; 2]> {
        self.project_view(self.to_view(p))
    }

    /// How many pixels one metre covers at camera-space depth `z`.
    pub fn pixels_per_metre(&self, z: f32) -> f32 {
        self.size[1] / (2.0 * z.max(NEAR) * self.tan_half_fov)
    }

    /// Whether a square of half-extent `half` around `center` touches the viewport.
    pub fn on_screen(&self, center: [f32; 2], half: f32) -> bool {
        center[0] + half >= 0.0
            && center[1] + half >= 0.0
            && center[0] - half <= self.size[0]
            && center[1] - half <= self.size[1]
    }
}

/// The part of the segment `a`→`b` in front of the near plane, as fractions `(t0, t1)` of the
/// segment, from the camera-space z of its ends.
pub fn clip_near(za: f32, zb: f32) -> Option<(f32, f32)> {
    match (za >= NEAR, zb >= NEAR) {
        (true, true) => Some((0.0, 1.0)),
        (false, false) => None,
        (true, false) => Some((0.0, (za - NEAR) / (za - zb))),
        (false, true) => Some(((NEAR - za) / (zb - za), 1.0)),
    }
}

/// Longest trail piece, as a fraction of its distance to the camera. ImGui interpolates texture
/// coordinates linearly on screen, not in perspective, so a quad that spans a big depth range
/// shows its texture warped and sliding as the camera moves. Short pieces keep that invisible.
const PIECE_DEPTH_RATIO: f32 = 0.08;
const MAX_PIECES: usize = 32;

/// How many pieces to cut a trail segment of `length` metres into, when its closest end is at
/// camera depth `nearest_z`.
pub fn trail_pieces(length: f32, nearest_z: f32) -> usize {
    let piece = PIECE_DEPTH_RATIO * nearest_z.max(NEAR);
    let pieces = (length / piece).ceil();
    if pieces.is_finite() {
        (pieces as usize).clamp(1, MAX_PIECES)
    } else {
        1
    }
}

/// The `"fov"` of MumbleLink's identity JSON, in radians.
pub fn parse_fov(identity: &str) -> Option<f32> {
    identity_number(identity, "fov")
}

/// The `"uisz"` (interface size: 0 small, 1 normal, 2 large, 3 larger) of the identity JSON.
pub fn parse_ui_size(identity: &str) -> Option<u32> {
    identity_number(identity, "uisz")
        .filter(|&n| (0.0..=3.0).contains(&n))
        .map(|n| n as u32)
}

/// A number of MumbleLink's flat identity JSON. A hand-rolled read, so the addon needs no JSON
/// parser for two numbers.
fn identity_number(identity: &str, key: &str) -> Option<f32> {
    let key = format!("\"{key}\"");
    let start = identity.find(&key)? + key.len();
    let rest = identity[start..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || "+-.eE".contains(c)))
        .unwrap_or(rest.len());
    rest[..end].parse().ok().filter(|f: &f32| f.is_finite())
}

/// What MumbleLink tells about the map on screen: the minimap, or the world map when it's open
/// (the same fields describe whichever is shown).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MapContext {
    pub map_open: bool,
    /// Compass size as MumbleLink gives it (before the interface size scale).
    pub compass_size: [f32; 2],
    pub compass_top_right: bool,
    pub compass_rotating: bool,
    /// Radians.
    pub compass_rotation: f32,
    /// Continent coordinates of the player and of the map's centre.
    pub player: [f32; 2],
    pub map_center: [f32; 2],
    /// Continent units per pixel at interface size 1.
    pub map_scale: f32,
    /// `uisz`: 0 small, 1 normal, 2 large, 3 larger.
    pub ui_size: u32,
    /// The character's position, metres.
    pub avatar: [f32; 3],
}

/// TacO's interface-size factor (`GetUIScale`).
pub fn ui_scale(ui_size: u32) -> f32 {
    match ui_size {
        0 => 0.9,
        2 => 1.111,
        3 => 1.224,
        _ => 1.0,
    }
}

/// TacO's `GetWindowTooSmallScale`: the game shrinks its interface below 1024×768.
fn too_small_scale([w, h]: [f32; 2]) -> f32 {
    if w < 1024.0 || h < 768.0 {
        (w / 1024.0).min(h / 768.0)
    } else {
        1.0
    }
}

/// Where map positions land on screen: TacO's MumbleLink-only transform (docs/05 §4.1–4.2,
/// `CompassData::BuildTransformationMatrix`). Assumes 1 continent unit = 24 game units.
#[derive(Debug, Clone, PartialEq)]
pub struct MapView {
    /// Top-left and bottom-right corners, pixels.
    pub rect: [[f32; 2]; 2],
    /// The character, in game units (x east, y north).
    origin: [f32; 2],
    rotation: f32,
    /// Continent units from the character's projection to the rect's centre.
    offset: [f32; 2],
    /// Pixels per continent unit.
    scale: f32,
    pub map_scale: f32,
    pub ui_scale: f32,
}

impl MapView {
    /// The view of the map on screen: the world map if it's open, else the minimap.
    pub fn new(ctx: &MapContext, screen: [f32; 2]) -> Option<Self> {
        if !(ctx.map_scale.is_finite() && ctx.map_scale > 0.0) {
            return None;
        }
        let ui = ui_scale(ctx.ui_size);
        let small = too_small_scale(screen);
        let [sw, sh] = screen;
        let (rect, rotation) = if ctx.map_open {
            ([[0.0, 0.0], [sw, sh]], 0.0)
        } else {
            let [w, h] = [ctx.compass_size[0] * ui, ctx.compass_size[1] * ui];
            if w < 1.0 || h < 1.0 {
                return None;
            }
            let (y1, y2) = if ctx.compass_top_right {
                (1.0, h * small + 1.0)
            } else {
                let delta = match ctx.ui_size {
                    0 => 33.0,
                    2 => 41.0,
                    3 => 45.0,
                    _ => 37.0,
                };
                (sh - h * small - delta * small, sh - delta * small)
            };
            let rotation = if ctx.compass_rotating {
                ctx.compass_rotation
            } else {
                0.0
            };
            ([[sw - w * small, y1], [sw, y2]], rotation)
        };
        let towards_center = [
            (ctx.map_center[0] - ctx.player[0]) * small,
            (ctx.map_center[1] - ctx.player[1]) * small,
        ];
        let [ox, oy] = rotate(towards_center, rotation);
        Some(Self {
            rect,
            origin: [
                ctx.avatar[0] * INCHES_PER_METRE,
                ctx.avatar[2] * INCHES_PER_METRE,
            ],
            rotation,
            offset: [-ox, -oy],
            scale: ui / ctx.map_scale,
            map_scale: ctx.map_scale,
            ui_scale: ui,
        })
    }

    /// Screen position of a map-space point (metres).
    pub fn project(&self, p: [f32; 3]) -> [f32; 2] {
        // Game units from the character, continent Y growing south.
        let d = [
            p[0] * INCHES_PER_METRE - self.origin[0],
            -(p[2] * INCHES_PER_METRE - self.origin[1]),
        ];
        let [dx, dy] = rotate(d, self.rotation);
        let [[x1, y1], [x2, y2]] = self.rect;
        [
            (x1 + x2) / 2.0 + (dx / 24.0 + self.offset[0]) * self.scale,
            (y1 + y2) / 2.0 + (dy / 24.0 + self.offset[1]) * self.scale,
        ]
    }

    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        let [[x1, y1], [x2, y2]] = self.rect;
        x >= x1 && x <= x2 && y >= y1 && y <= y2
    }
}

/// TacO's `CVector2::Rotated` around the origin.
fn rotate([x, y]: [f32; 2], r: f32) -> [f32; 2] {
    let (sin, cos) = r.sin_cos();
    [x * cos - y * sin, y * cos + x * sin]
}

/// Where an item is drawn: `inGameVisibility`, `mapVisibility`, `miniMapVisibility` (all
/// default to visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visibility {
    pub world: bool,
    pub map: bool,
    pub minimap: bool,
}

impl Visibility {
    /// The attributes, in the order of the fields.
    pub const ATTRS: [&str; 3] = ["inGameVisibility", "mapVisibility", "miniMapVisibility"];

    fn of<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Self {
        let flag = |name| get(name).is_none_or(parse_flag);
        Self {
            world: flag("inGameVisibility"),
            map: flag("mapVisibility"),
            minimap: flag("miniMapVisibility"),
        }
    }
}

/// A boolean attribute as TacO reads them (`atoi(value) != 0`; `true`/`false` also accepted).
pub fn parse_flag(value: &str) -> bool {
    let value = value.trim();
    if value.eq_ignore_ascii_case("true") {
        return true;
    }
    if value.eq_ignore_ascii_case("false") {
        return false;
    }
    let digits: String = value
        .chars()
        .enumerate()
        .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))
        .map(|(_, c)| c)
        .collect();
    digits.parse::<i64>().is_ok_and(|n| n != 0)
}

/// How a marker or trail looks on the minimap and world map (docs/05 §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapStyle {
    /// `mapDisplaySize`, pixels at map scale 1.
    pub size: f32,
    /// `scaleOnMapWithZoom`: shrink when zoomed out.
    pub scale_with_zoom: bool,
    /// `mapFadeoutScaleLevel`.
    pub fadeout_level: f32,
}

impl MapStyle {
    fn of<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Self {
        Self {
            size: float(get("mapDisplaySize"), 20.0).max(0.0),
            scale_with_zoom: get("scaleOnMapWithZoom").is_none_or(parse_flag),
            fadeout_level: float(get("mapFadeoutScaleLevel"), 100.0),
        }
    }

    /// Size in pixels and opacity factor on `view` (TacO `DrawPOIMinimap`).
    pub fn on(&self, view: &MapView) -> (f32, f32) {
        let mut size = self.size;
        if self.scale_with_zoom {
            size /= view.map_scale;
        }
        let fade = 1.0 - ((view.map_scale - self.fadeout_level) / 2.0).clamp(0.0, 1.0);
        (size * view.ui_scale, fade)
    }
}

/// How a marker is drawn, from its own attributes or else its categories'.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerStyle {
    /// `iconFile`, relative to the pack root. `None`: the overlay's default icon.
    pub icon: Option<String>,
    /// Metres above `ypos` the icon is drawn.
    pub height_offset: f32,
    /// The icon's world size is 2 m × this (docs/05 §2.1).
    pub icon_size: f32,
    /// Bounds of the icon's half-size on screen, in pixels.
    pub min_size: f32,
    pub max_size: f32,
    pub alpha: f32,
    /// `fadeNear`, `fadeFar` in metres, when both are set.
    pub fade: Option<(f32, f32)>,
    pub visible: Visibility,
    pub map: MapStyle,
}

impl MarkerStyle {
    pub fn of(pack: &Pack, poi: &Poi) -> Self {
        let get = |name: &str| pack.inherited(&poi.category, &poi.attrs, name);
        let float = |name, default| float(get(name), default);
        Self {
            icon: get("iconFile").map(str::to_owned),
            height_offset: float("heightOffset", 1.5),
            icon_size: float("iconSize", 1.0).max(0.0),
            min_size: float("minSize", 5.0).max(0.0),
            max_size: float("maxSize", 2048.0).max(1.0),
            alpha: float("alpha", 1.0).clamp(0.0, 1.0),
            fade: fade(get("fadeNear"), get("fadeFar")),
            visible: Visibility::of(get),
            map: MapStyle::of(get),
        }
    }
}

/// How a trail is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct TrailStyle {
    /// `texture`, relative to the pack root. `None`: the overlay's default texture.
    pub texture: Option<String>,
    /// Half the ribbon's width in metres.
    pub half_width: f32,
    pub alpha: f32,
    pub fade: Option<(f32, f32)>,
    pub visible: Visibility,
    pub map: MapStyle,
}

impl TrailStyle {
    pub fn of(pack: &Pack, trail: &Trail) -> Self {
        let get = |name: &str| pack.inherited(&trail.category, &trail.attrs, name);
        Self {
            texture: get("texture").map(str::to_owned),
            half_width: TRAIL_HALF_WIDTH_M * float(get("trailScale"), 1.0).clamp(0.05, 20.0),
            alpha: float(get("alpha"), 1.0).clamp(0.0, 1.0),
            fade: fade(get("fadeNear"), get("fadeFar")),
            visible: Visibility::of(get),
            map: MapStyle::of(get),
        }
    }
}

/// Opacity factor at `distance` metres from the camera, or `None` past `fadeFar` (docs/05
/// §2.2).
pub fn fade_factor(fade: Option<(f32, f32)>, distance: f32) -> Option<f32> {
    let Some((near, far)) = fade else {
        return Some(1.0);
    };
    if distance > far {
        return None;
    }
    if far <= near {
        return Some(1.0);
    }
    Some(1.0 - ((distance - near) / (far - near)).clamp(0.0, 1.0))
}

fn float(value: Option<&str>, default: f32) -> f32 {
    value
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// Both distances in metres, if both are set and not negative (negative disables the fade).
fn fade(near: Option<&str>, far: Option<&str>) -> Option<(f32, f32)> {
    let near = float(near, -1.0);
    let far = float(far, -1.0);
    (near >= 0.0 && far >= 0.0).then(|| (near / INCHES_PER_METRE, far / INCHES_PER_METRE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{attrs, pack::Category};

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
    }

    /// At the origin, looking north, 90° vertical FOV, 200×100 viewport.
    fn camera() -> Camera {
        Camera::new(
            [0.0; 3],
            [0.0, 0.0, 1.0],
            std::f32::consts::FRAC_PI_2,
            [200.0, 100.0],
        )
        .unwrap()
    }

    #[test]
    fn projection() {
        let cam = camera();
        // Straight ahead: the centre.
        assert!(close(cam.project([0.0, 0.0, 10.0]).unwrap(), [100.0, 50.0]));
        // Up and east (left-handed, Y up, Z north: east is to the right).
        assert!(close(cam.project([0.0, 10.0, 10.0]).unwrap(), [100.0, 0.0]));
        assert!(close(
            cam.project([20.0, 0.0, 10.0]).unwrap(),
            [200.0, 50.0]
        ));
        // Behind.
        assert_eq!(cam.project([0.0, 0.0, -1.0]), None);
        // 100 px tall at 90°: at 10 m, the half-height (10 m) covers 50 px.
        assert!((cam.pixels_per_metre(10.0) - 5.0).abs() < 1e-4);

        // Looking east: north is to the left.
        let east = Camera::new([0.0; 3], [1.0, 0.0, 0.0], 1.0, [200.0, 100.0]).unwrap();
        assert!(east.project([10.0, 0.0, 5.0]).unwrap()[0] < 100.0);
        // A camera can't be built from nonsense.
        assert!(Camera::new([0.0; 3], [0.0; 3], 1.0, [1.0, 1.0]).is_none());
        assert!(Camera::new([0.0; 3], [0.0, 1.0, 0.0], 1.0, [1.0, 1.0]).is_none());
        assert!(Camera::new([0.0; 3], [0.0, 0.0, 1.0], 0.0, [1.0, 1.0]).is_none());
    }

    #[test]
    fn near_clipping() {
        assert_eq!(clip_near(1.0, 2.0), Some((0.0, 1.0)));
        assert_eq!(clip_near(-1.0, -2.0), None);
        let (t0, t1) = clip_near(-0.9, 1.1).unwrap();
        assert!((t0 - 0.5).abs() < 1e-6 && t1 == 1.0);
        let (t0, t1) = clip_near(1.1, -0.9).unwrap();
        assert!(t0 == 0.0 && (t1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn pieces() {
        assert_eq!(trail_pieces(1.0, 100.0), 1, "far away: one piece");
        assert_eq!(trail_pieces(1.0, 5.0), 3);
        assert_eq!(trail_pieces(1.0, 0.0), MAX_PIECES, "at the camera: capped");
        assert_eq!(trail_pieces(f32::NAN, 1.0), 1);
    }

    #[test]
    fn fov_from_identity() {
        let identity = r#"{"name":"Example Character","profession":4,"spec":55,"race":4,"map_id":50,"world_id":1001,"team_color_id":0,"commander":false,"fov":0.873,"uisz":1}"#;
        assert_eq!(parse_fov(identity), Some(0.873));
        assert_eq!(parse_fov(r#"{"fov" : 1.2}"#), Some(1.2));
        assert_eq!(parse_fov(r#"{"name":"x"}"#), None);
        assert_eq!(parse_fov(""), None);
    }

    #[test]
    fn styles_are_inherited() {
        let mut pack = Pack::default();
        pack.categories.push(Category {
            name: "a".into(),
            attrs: vec![
                ("iconFile".into(), "images/a.png".into()),
                ("heightOffset".into(), "3".into()),
                ("fadeNear".into(), "3937".into()),
                ("fadeFar".into(), "7874".into()),
                ("texture".into(), "images/t.png".into()),
            ],
            children: vec![Category {
                name: "b".into(),
                attrs: vec![("iconSize".into(), "2".into())],
                children: Vec::new(),
            }],
        });
        let mut poi = Poi::new(50, [0.0; 3], "a.b");
        let style = MarkerStyle::of(&pack, &poi);
        assert_eq!(style.icon.as_deref(), Some("images/a.png"));
        assert_eq!((style.height_offset, style.icon_size), (3.0, 2.0));
        let (near, far) = style.fade.unwrap();
        assert!((near - 100.0).abs() < 0.01 && (far - 200.0).abs() < 0.01);

        // Own attributes win.
        attrs::set(&mut poi.attrs, "iconFile", "images/own.png");
        attrs::set(&mut poi.attrs, "fadeFar", "-1");
        let style = MarkerStyle::of(&pack, &poi);
        assert_eq!(style.icon.as_deref(), Some("images/own.png"));
        assert_eq!(style.fade, None, "a negative distance disables the fade");

        // Defaults.
        let style = MarkerStyle::of(&pack, &Poi::new(50, [0.0; 3], "other"));
        assert_eq!(style.icon, None);
        assert_eq!(
            (style.height_offset, style.icon_size, style.alpha),
            (1.5, 1.0, 1.0)
        );

        let trail = Trail::new(50, "a.b", "trails/x.trl");
        let style = TrailStyle::of(&pack, &trail);
        assert_eq!(style.texture.as_deref(), Some("images/t.png"));
        // Trail::new writes fadeNear 5000 / fadeFar 5100 on the trail itself.
        assert!((style.fade.unwrap().1 - 5100.0 / 39.37).abs() < 0.01);
    }

    #[test]
    fn identity_numbers() {
        let identity = r#"{"name":"Example Character","fov":0.873,"uisz":2}"#;
        assert_eq!(parse_ui_size(identity), Some(2));
        assert_eq!(parse_ui_size(r#"{"uisz":7}"#), None);
    }

    fn context() -> MapContext {
        MapContext {
            map_open: false,
            compass_size: [200.0, 150.0],
            compass_top_right: false,
            compass_rotating: false,
            compass_rotation: 0.0,
            player: [1000.0, 2000.0],
            map_center: [1000.0, 2000.0],
            map_scale: 2.0,
            ui_size: 1,
            avatar: [0.0; 3],
        }
    }

    #[test]
    fn minimap_projection() {
        let screen = [1920.0, 1080.0];
        let view = MapView::new(&context(), screen).unwrap();
        // Bottom right, above the 37 px bar at normal interface size.
        assert_eq!(view.rect, [[1720.0, 893.0], [1920.0, 1043.0]]);
        let center = [1820.0, 968.0];
        assert!(
            close(view.project([0.0; 3]), center),
            "the character is at the centre"
        );
        // 24 game units = 1 continent unit = 1/map_scale px; east is right, north is up.
        let unit = 24.0 / INCHES_PER_METRE;
        assert!(close(
            view.project([unit * 10.0, 0.0, 0.0]),
            [center[0] + 5.0, center[1]]
        ));
        assert!(close(
            view.project([0.0, 50.0, unit * 10.0]),
            [center[0], center[1] - 5.0]
        ));
        assert!(view.contains(center) && !view.contains([0.0, 0.0]));

        // Panned: the map centre is 10 units east of the player, so the player is left of it.
        let mut ctx = context();
        ctx.map_center[0] += 10.0;
        let view = MapView::new(&ctx, screen).unwrap();
        assert!(close(view.project([0.0; 3]), [center[0] - 5.0, center[1]]));

        // Rotating compass, top right.
        ctx = context();
        ctx.compass_top_right = true;
        ctx.compass_rotating = true;
        ctx.compass_rotation = std::f32::consts::FRAC_PI_2;
        let view = MapView::new(&ctx, screen).unwrap();
        assert_eq!(view.rect, [[1720.0, 1.0], [1920.0, 151.0]]);
        let east = view.project([unit * 10.0, 0.0, 0.0]);
        assert!(close(east, [1820.0, 76.0 + 5.0]), "{east:?}");

        // World map: the whole screen, never rotated.
        ctx.map_open = true;
        let view = MapView::new(&ctx, screen).unwrap();
        assert_eq!(view.rect, [[0.0, 0.0], screen]);
        assert!(close(view.project([unit * 10.0, 0.0, 0.0]), [965.0, 540.0]));

        ctx.map_scale = 0.0;
        assert!(MapView::new(&ctx, screen).is_none());
    }

    #[test]
    fn visibility_and_map_style() {
        let mut pack = Pack::default();
        pack.categories.push(Category {
            name: "a".into(),
            attrs: vec![
                ("miniMapVisibility".into(), "0".into()),
                ("mapDisplaySize".into(), "40".into()),
            ],
            children: Vec::new(),
        });
        let mut poi = Poi::new(50, [0.0; 3], "a");
        poi.attrs.push(("inGameVisibility".into(), "false".into()));
        let style = MarkerStyle::of(&pack, &poi);
        assert_eq!(
            style.visible,
            Visibility {
                world: false,
                map: true,
                minimap: false
            }
        );
        let view = MapView::new(&context(), [1920.0, 1080.0]).unwrap();
        assert_eq!(style.map.on(&view), (20.0, 1.0), "40 px / map scale 2");

        assert!(parse_flag("1") && parse_flag(" 2 ") && parse_flag("TRUE"));
        assert!(!parse_flag("0") && !parse_flag("no") && !parse_flag(""));
    }

    #[test]
    fn fading() {
        assert_eq!(fade_factor(None, 1e6), Some(1.0));
        let fade = Some((10.0, 20.0));
        assert_eq!(fade_factor(fade, 5.0), Some(1.0));
        assert_eq!(fade_factor(fade, 15.0), Some(0.5));
        assert_eq!(fade_factor(fade, 25.0), None);
    }
}
