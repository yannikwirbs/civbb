//! glTF animations to clips: translation, rotation, scale and morph weight channels as Hermite keys, within the caps.

use std::collections::HashMap;

use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::hierarchy::NodeIndex;
use gltf::animation::{Interpolation, Property as GltfProperty};

use super::Importer;
use crate::accessor::floats;

impl Importer<'_> {
    pub(super) fn animations(&mut self) -> Result<Vec<Clip>, String> {
        let doc = self.doc;
        let (mut curves_left, mut keys_left) = (self.caps.curves, self.caps.keys);
        let mut paths: HashMap<NodeIndex, String> = HashMap::new();
        let mut clips = Vec::new();
        for (ai, anim) in doc.animations().enumerate() {
            let mut clip = Clip {
                name: anim.name().map_or_else(|| format!("animation{ai}"), str::to_owned),
                ..Clip::default()
            };
            for ch in anim.channels() {
                let err = |e: String| format!("animation {ai} channel {}: {e}", ch.index());
                let Some(at) = self.map[ch.target().node().index()] else {
                    self.warnings.push(format!(
                        "animation {ai}: channel {} targets a node outside the scene",
                        ch.index()
                    ));
                    continue;
                };
                let properties: Vec<Property> = match ch.target().property() {
                    GltfProperty::Translation => (0..3).map(Property::Position).collect(),
                    GltfProperty::Rotation => (0..4).map(Property::Rotation).collect(),
                    GltfProperty::Scale => (0..3).map(Property::Scale).collect(),
                    GltfProperty::MorphTargetWeights => match self.node_meshes.get(&at) {
                        Some(&mi) => self.meshes[mi]
                            .blend_shapes
                            .iter()
                            .map(|s| Property::BlendShape(s.name.clone()))
                            .collect(),
                        None => return Err(err("morph weights on a node without a mesh".into())),
                    },
                };
                let s = ch.sampler();
                let times = floats(&s.input(), &self.buffers, keys_left).map_err(err)?;
                if s.input().dimensions().multiplicity() != 1 {
                    return Err(err("input is not scalar".into()));
                }
                if !times.iter().all(|t| t.is_finite()) || !times.windows(2).all(|w| w[0] < w[1]) {
                    return Err(err("key times not finite and strictly increasing".into()));
                }
                let cubic = s.interpolation() == Interpolation::CubicSpline;
                let stride = properties.len() * if cubic { 3 } else { 1 };
                let max = times.len().saturating_mul(stride);
                let values = floats(&s.output(), &self.buffers, max).map_err(err)?;
                if values.len() != max {
                    return Err(err("output count does not match input".into()));
                }
                let used = times.len() * properties.len();
                keys_left = keys_left
                    .checked_sub(used)
                    .ok_or_else(|| err("key cap exceeded".into()))?;
                curves_left = curves_left
                    .checked_sub(properties.len())
                    .ok_or_else(|| err("curve cap exceeded".into()))?;
                let path = match paths.get(&at) {
                    Some(p) => p.clone(),
                    None => {
                        let p = self.path(at).map_err(err)?;
                        paths.insert(at, p.clone());
                        p
                    }
                };
                // glTF weights are 0 to 1; blendshape curves use Unity's 0 to 100.
                let scale = if matches!(ch.target().property(), GltfProperty::MorphTargetWeights) {
                    100.0
                } else {
                    1.0
                };
                for (c, property) in properties.into_iter().enumerate() {
                    let keys = keys(&times, &values, c, stride, s.interpolation(), scale);
                    clip.curves.push(Curve {
                        binding: Binding {
                            path: path.clone(),
                            property,
                        },
                        keys,
                    });
                }
                clip.length = clip.length.max(times.last().copied().unwrap_or(0.0));
            }
            if !clip.curves.is_empty() {
                clips.push(clip);
            }
        }
        if clips.len() > self.caps.clips {
            return Err(format!("{} clips exceeds cap {}", clips.len(), self.caps.clips));
        }
        Ok(clips)
    }
}

/// Hermite keys for component `c` of a sampler. Linear keys get the segment slopes, step keys infinite tangents.
fn keys(times: &[f32], values: &[f32], c: usize, stride: usize, interp: Interpolation, scale: f32) -> Vec<Key> {
    let cubic = interp == Interpolation::CubicSpline;
    let comps = if cubic { stride / 3 } else { stride };
    // Cubic spline outputs are (in tangent, value, out tangent) triples per key.
    let at = |k: usize, part: usize| values[k * stride + part * comps + c] * scale;
    let value = |k: usize| at(k, if cubic { 1 } else { 0 });
    let slope = |a: usize, b: usize| (value(b) - value(a)) / (times[b] - times[a]);
    (0..times.len())
        .map(|k| {
            let (in_tangent, out_tangent) = match interp {
                Interpolation::Step => (f32::INFINITY, f32::INFINITY),
                Interpolation::CubicSpline => (at(k, 0), at(k, 2)),
                Interpolation::Linear => (
                    if k > 0 { slope(k - 1, k) } else { 0.0 },
                    if k + 1 < times.len() { slope(k, k + 1) } else { 0.0 },
                ),
            };
            Key {
                time: times[k],
                value: value(k),
                in_tangent,
                out_tangent,
            }
        })
        .collect()
}
