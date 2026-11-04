//! FLIP parity: a test avatar and a reference rendered with the same camera rig, states and poses, compared per view
//! with NVIDIA FLIP and scored as a traffic light (design doc §10, ADR 0033).

use std::path::Path;
use std::sync::Arc;

use civbb_core::math::Pose;
use civbb_format::Avatar;
use civbb_format::cost::{Light, Parity, Rig, RigView, StateParity, Thresholds, ViewScore};
use civbb_format::math::{Mat4, Vec3};
use civbb_render::Renderer;

use crate::frame;
use crate::island::{Island, Model};
use crate::view::{self, BACKGROUND, Gpu, Scratch, TextureImage};

/// Rig image size in pixels: square, as T-posed avatars are about as wide as tall.
pub const WIDTH: u32 = 640;
/// Rig image size in pixels.
pub const HEIGHT: u32 = 640;
/// The rig's vertical field of view in radians.
pub const FOV_Y: f32 = 0.6;
/// FLIP's default: a 0.7 m viewing distance from a 3840-pixel-wide 0.7 m display.
pub const PIXELS_PER_DEGREE: f32 = flip_rs::DEFAULT_PPD;
/// The traffic light's bounds (ADR 0033).
pub const THRESHOLDS: Thresholds = Thresholds {
    green_mean: 0.02,
    green_p99: 0.2,
    red_mean: 0.1,
    red_p99: 0.5,
};
/// Simulation frames stepped per state, at 60 Hz.
pub const FRAMES_PER_STATE: u32 = 3;

/// One side of a comparison: a validated avatar and its decoded textures.
pub struct Side<'a> {
    /// The avatar.
    pub avatar: &'a Avatar,
    /// Its textures, from [`view::decode_textures`].
    pub textures: &'a [Option<TextureImage>],
}

/// Steps one island per avatar through the same states: rest, then each of the first avatar's parameters set away
/// from its default (0 to 1, anything else to 0) on top of the state before, [`FRAMES_PER_STATE`] frames each.
/// Calls `check` with the state number, its name (`rest`, `Hat=1`) and the islands after each.
pub fn states(avatars: &[&Avatar], mut check: impl FnMut(usize, &str, &[Island])) {
    let mut islands: Vec<Island> = avatars
        .iter()
        .map(|a| Island::new(Arc::new(Model::new(a)), 1, true, Pose::IDENTITY))
        .collect();
    let params: Vec<(String, f32)> = avatars[0]
        .animator
        .iter()
        .flat_map(|x| &x.parameters)
        .map(|p| (p.name.clone(), if p.default == 0.0 { 1.0 } else { 0.0 }))
        .collect();
    for state in 0..=params.len() {
        for island in &mut islands {
            if state > 0 {
                let (p, v) = &params[state - 1];
                let i = island.model().controller().and_then(|c| c.param(p));
                if let (Some(i), Some(animator)) = (i, island.animator_mut()) {
                    animator.params[i as usize] = *v;
                }
            }
            for _ in 0..FRAMES_PER_STATE {
                frame::step(std::slice::from_mut(island), |_, _| true, 1.0 / 60.0);
            }
        }
        let name = match state {
            0 => "rest".to_owned(),
            s => format!("{}={}", params[s - 1].0, params[s - 1].1),
        };
        check(state, &name, &islands);
    }
}

/// The rig for an avatar: front, left, back and right, level with the centre of its [`rest_bounds`], each as far
/// away as the bounds need to fill 90 % of the image (height or width across the view) from their near side.
pub fn rig(avatar: &Avatar) -> Rig {
    let (lo, hi) = rest_bounds(avatar);
    let centre: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
    let size: [f32; 3] = std::array::from_fn(|k| (hi[k] - lo[k]).max(0.01));
    let tan_y = (FOV_Y / 2.0).tan();
    let tan_x = tan_y * WIDTH as f32 / HEIGHT as f32;
    let views = [("front", 0.0_f32), ("left", 90.0), ("back", 180.0), ("right", 270.0)]
        .into_iter()
        .map(|(name, yaw)| {
            // Across and along the view: X and Z from the front and back, Z and X from the sides.
            let (across, along) = if yaw % 180.0 == 0.0 {
                (size[0], size[2])
            } else {
                (size[2], size[0])
            };
            let distance = (size[1] * 0.55 / tan_y).max(across * 0.55 / tan_x) + along / 2.0;
            let (s, c) = yaw.to_radians().sin_cos();
            RigView {
                name: name.into(),
                eye: [centre[0] + distance * s, centre[1], centre[2] + distance * c],
                target: centre,
            }
        })
        .collect();
    Rig {
        width: WIDTH,
        height: HEIGHT,
        fov_y: FOV_Y,
        background: [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2]],
        views,
    }
}

/// The world-space bounds of what the avatar shows at rest: the vertices of meshes on active nodes after
/// [`FRAMES_PER_STATE`] frames, skinned and with shape weights applied. All zero when nothing shows.
pub fn rest_bounds(avatar: &Avatar) -> (Vec3, Vec3) {
    let mut island = Island::new(Arc::new(Model::new(avatar)), 1, true, Pose::IDENTITY);
    for _ in 0..FRAMES_PER_STATE {
        frame::step(std::slice::from_mut(&mut island), |_, _| true, 1.0 / 60.0);
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (mesh, vertices) in avatar.meshes.iter().zip(placed(avatar, &island)) {
        if island.active()[mesh.node as usize] {
            for p in vertices {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
    }
    if lo[0] > hi[0] { ([0.0; 3], [0.0; 3]) } else { (lo, hi) }
}

/// Every mesh's vertices in world space after the island's shape weights and skinning, as the GPU places them.
pub fn placed(avatar: &Avatar, island: &Island) -> Vec<Vec<Vec3>> {
    let mut weights = Vec::new();
    island.shape_weights(&mut weights);
    let world = island.world();
    let mut out = Vec::new();
    for (mesh, weights) in avatar.meshes.iter().zip(&weights) {
        let mut pos = mesh.positions.clone();
        for (s, &w) in mesh.blend_shapes.iter().zip(weights) {
            if w != 0.0 {
                for d in &s.deltas {
                    (0..3).for_each(|k| pos[d.vertex as usize][k] += w * d.position[k]);
                }
            }
        }
        let placed: Vec<Vec3> = match &mesh.skin {
            Some(s) if !s.joints.is_empty() => pos
                .iter()
                .zip(&s.vertex_joints)
                .zip(&s.vertex_weights)
                .map(|((p, js), ws)| {
                    let mut v = [0.0; 3];
                    for (&j, &w) in js.iter().zip(ws).filter(|(_, w)| **w != 0.0) {
                        let q = world[s.joints[j as usize] as usize].point(transform(&s.inverse_bind[j as usize], *p));
                        (0..3).for_each(|k| v[k] += w * q[k]);
                    }
                    v
                })
                .collect(),
            _ => pos.iter().map(|p| world[mesh.node as usize].point(*p)).collect(),
        };
        // Signed zeros compare equal but sort apart.
        out.push(placed.iter().map(|p| p.map(|x| x + 0.0)).collect());
    }
    out
}

/// A column-major matrix applied to a point.
fn transform(m: &Mat4, p: Vec3) -> Vec3 {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

/// Renders `test` and `reference` with the reference's [`rig`] in every [`states`] state and scores each view. With
/// `images`, writes `<state>-<view>.png` there (reference, test and the FLIP error map side by side) for the rest
/// state and every state that is not green; a failed write is reported on stderr. Fails with
/// [`civbb_render::Error::NoDevice`] or [`civbb_render::Error::Load`] where there is no device.
pub fn run(reference: Side, test: Side, images: Option<&Path>) -> civbb_render::Result<Parity> {
    let rig = rig(reference.avatar);
    let mut renderer = Renderer::headless(cfg!(debug_assertions), WIDTH, HEIGHT)?;
    let sides = [&reference, &test];
    let mut gpus = Vec::with_capacity(2);
    for side in sides {
        match Gpu::new(&renderer, side.avatar, &Model::new(side.avatar), side.textures, 1) {
            Ok(gpu) => gpus.push(gpu),
            Err(e) => {
                destroy(&renderer, gpus);
                return Err(e);
            }
        }
    }
    let mut scratch = [Scratch::default(), Scratch::default()];
    let mut scored = Vec::new();
    let mut failed = None;
    if let Some(dir) = images
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("civbb-client: {}: {e}", dir.display());
    }
    states(&[reference.avatar, test.avatar], |state, name, islands| {
        if failed.is_some() {
            return;
        }
        let mut shots = Vec::with_capacity(rig.views.len());
        let mut views = Vec::with_capacity(rig.views.len());
        for v in &rig.views {
            let mut pair = Vec::with_capacity(2);
            for (k, side) in sides.iter().enumerate() {
                let drawn = view::draw(
                    &mut renderer,
                    &gpus[k],
                    side.avatar,
                    std::slice::from_ref(&islands[k]),
                    &mut scratch[k],
                    v.eye,
                    v.target,
                    FOV_Y,
                    BACKGROUND,
                )
                .and_then(|_| renderer.read_last());
                match drawn {
                    Ok(texels) => pair.push(texels),
                    Err(e) => {
                        failed = Some(e);
                        return;
                    }
                }
            }
            let (score, error) = compare(&pair[0], &pair[1], WIDTH, HEIGHT);
            views.push(score);
            shots.push((pair, error));
        }
        let light = views.iter().map(|s| light(s, &THRESHOLDS)).max().unwrap_or_default();
        if let Some(dir) = images
            && (state == 0 || light != Light::Green)
        {
            for (v, (pair, error)) in rig.views.iter().zip(&shots) {
                let path = dir.join(format!("{state:03}-{}.png", v.name));
                if let Err(e) = side_by_side(&pair[0], &pair[1], error).save(&path) {
                    eprintln!("civbb-client: {}: {e}", path.display());
                }
            }
        }
        scored.push(StateParity {
            name: name.to_owned(),
            light,
            views,
        });
    });
    destroy(&renderer, gpus);
    if let Some(e) = failed {
        return Err(e);
    }
    // Validation runs in debug builds only; an error there is a renderer bug, not the avatar's.
    let errors = renderer.context().validation_errors();
    assert!(errors == 0, "{errors} Vulkan validation errors");
    Ok(Parity {
        reference: "source".into(),
        rig,
        pixels_per_degree: PIXELS_PER_DEGREE,
        thresholds: THRESHOLDS,
        light: scored.iter().map(|s| s.light).max().unwrap_or_default(),
        states: scored,
    })
}

fn destroy(renderer: &Renderer, gpus: Vec<Gpu>) {
    // SAFETY: after the wait nothing is in flight.
    unsafe {
        let _ = renderer.context().device().device_wait_idle();
        for gpu in gpus {
            gpu.destroy(renderer);
        }
    }
}

/// FLIP of two `width` by `height` images of [`civbb_render::HEADLESS_FORMAT`] texels (BGRA8, sRGB-encoded): the
/// score over the pixels that are not the rig's background in either image, and the full error map.
pub fn compare(reference: &[u8], test: &[u8], width: u32, height: u32) -> (ViewScore, Vec<f32>) {
    let background = background_texel();
    let near = |px: &[u8]| (0..3).all(|k| px[k].abs_diff(background[k]) <= 1);
    let covered: Vec<bool> = reference
        .as_chunks::<4>()
        .0
        .iter()
        .zip(test.as_chunks::<4>().0.iter())
        .map(|(r, t)| !near(r) || !near(t))
        .collect();
    let error = flip(&rgb(reference), &rgb(test), width, height);
    let mut errors: Vec<f32> = error
        .iter()
        .zip(&covered)
        .filter(|(_, c)| **c)
        .map(|(e, _)| *e)
        .collect();
    (score(&mut errors), error)
}

/// FLIP's error map of two sRGB RGB8 images, rows from the top; 0 where equal, up to 1.
pub fn flip(reference: &[u8], test: &[u8], width: u32, height: u32) -> Vec<f32> {
    let image = |px: &[u8]| flip_rs::RgbImage::new(width as usize, height as usize, px.to_vec()).expect("RGB8 size");
    flip_rs::ldr_flip(&image(reference), &image(test), PIXELS_PER_DEGREE)
        .expect("8-bit input and the default viewing distance are valid")
        .into_pixels()
}

/// Mean, 99th percentile, largest and count of `errors` (sorted in place); zeros when empty.
pub fn score(errors: &mut [f32]) -> ViewScore {
    if errors.is_empty() {
        return ViewScore::default();
    }
    errors.sort_by(f32::total_cmp);
    let n = errors.len();
    ViewScore {
        mean: (errors.iter().map(|&e| f64::from(e)).sum::<f64>() / n as f64) as f32,
        p99: errors[((n - 1) as f64 * 0.99).round() as usize],
        max: errors[n - 1],
        pixels: n as u32,
    }
}

/// A view's light under `t`.
pub fn light(s: &ViewScore, t: &Thresholds) -> Light {
    if s.mean > t.red_mean || s.p99 > t.red_p99 {
        Light::Red
    } else if s.mean > t.green_mean || s.p99 > t.green_p99 {
        Light::Yellow
    } else {
        Light::Green
    }
}

/// The background colour as the headless target stores it: linear to sRGB-encoded bytes.
fn background_texel() -> [u8; 3] {
    let encode = |c: f32| {
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    [encode(BACKGROUND[2]), encode(BACKGROUND[1]), encode(BACKGROUND[0])]
}

/// BGRA8 texels to RGB8.
fn rgb(bgra: &[u8]) -> Vec<u8> {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0]])
        .collect()
}

/// Reference, test and the error map in FLIP's magma colours, side by side.
fn side_by_side(reference: &[u8], test: &[u8], error: &[f32]) -> image::RgbImage {
    let (r, t) = (rgb(reference), rgb(test));
    image::RgbImage::from_fn(WIDTH * 3, HEIGHT, |x, y| {
        let (panel, x) = (x / WIDTH, x % WIDTH);
        let i = (y * WIDTH + x) as usize;
        match panel {
            0 => image::Rgb([r[3 * i], r[3 * i + 1], r[3 * i + 2]]),
            1 => image::Rgb([t[3 * i], t[3 * i + 1], t[3 * i + 2]]),
            _ => {
                let c = flip_rs::MAGMA[(error[i].clamp(0.0, 1.0) * 255.0).round() as usize];
                image::Rgb(c.map(|v| (v * 255.0).round() as u8))
            }
        }
    })
}
