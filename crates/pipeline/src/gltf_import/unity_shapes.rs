//! Unity dialect 1's JSON shapes rewritten into the native ones (ADRs 0086, 0102): dialect 1 is the format types'
//! serde JSON of format v25 (externally tagged enums, Rust variant names, tuples), which the native types no longer
//! read. Each function rewrites the positions of one `X_*` part that hold native types; anything unexpected is left
//! as it is for the parse to reject.

use serde_json::{Map, Value, json};

/// A Rust variant name as the native JSON spells it.
fn snake(name: &str) -> String {
    match name {
        "Simple1D" => return "simple_1d".into(),
        "SimpleDirectional2D" => return "simple_directional_2d".into(),
        "FreeformDirectional2D" => return "freeform_directional_2d".into(),
        "FreeformCartesian2D" => return "freeform_cartesian_2d".into(),
        "Version_1_0" => return "1.0".into(),
        "Version_1_1" => return "1.1".into(),
        _ => {}
    }
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A unit-only enum's value.
fn unit(v: &mut Value) {
    if let Value::String(s) = v {
        *s = snake(s);
    }
}

/// An externally tagged enum's value as an internally tagged one; `field` names the field of a variant whose payload
/// is not an object.
fn tagged(v: &mut Value, field: impl Fn(&str) -> &'static str) {
    let new = match v {
        Value::String(s) => json!({"type": snake(s)}),
        Value::Object(o) if o.len() == 1 => {
            let (k, p) = o.iter().next().expect("one entry");
            let mut out = Map::from_iter([("type".to_owned(), json!(snake(k)))]);
            match p {
                Value::Object(fields) => out.extend(fields.clone()),
                p => {
                    out.insert(field(k).into(), p.clone());
                }
            }
            Value::Object(out)
        }
        _ => return,
    };
    *v = new;
}

fn plain(_: &str) -> &'static str {
    "value"
}

/// Each element of the array at `key`, if any.
fn each(v: &mut Value, key: &str, mut f: impl FnMut(&mut Value)) {
    if let Some(a) = v.get_mut(key).and_then(Value::as_array_mut) {
        a.iter_mut().for_each(&mut f);
    }
}

/// `X_avatar.face`.
pub(super) fn face(f: &mut Value) {
    each(f, "visemes", |v| {
        if let Some([name, binds]) = v.as_array().and_then(|a| <&[Value; 2]>::try_from(a.as_slice()).ok()) {
            let mut name = name.clone();
            unit(&mut name);
            *v = json!({"viseme": name, "binds": binds});
        }
    });
    if let Some(d) = f.get_mut("eyes").and_then(|e| e.get_mut("driver")) {
        tagged(d, plain);
    }
}

/// `X_animator.animator`, before its states are lowered.
pub(super) fn animator(a: &mut Value) {
    each(a, "parameters", |p| {
        if let Some(k) = p.get_mut("kind") {
            unit(k);
        }
    });
    each(a, "layers", |l| {
        if let Some(b) = l.get_mut("blending") {
            unit(b);
        }
        if let Some(m) = l.get_mut("mask").filter(|m| m.is_object()) {
            each(m, "body_parts", unit);
        }
        let Some(sm) = l.get_mut("state_machine") else {
            return;
        };
        each(sm, "entry_transitions", transition);
        each(sm, "any_state_transitions", transition);
        each(sm, "states", |s| {
            if let Some(m) = s.get_mut("motion").filter(|m| !m.is_null()) {
                motion(m);
            }
            each(s, "transitions", transition);
            each(s, "behaviours", behaviour);
        });
    });
}

fn motion(m: &mut Value) {
    tagged(m, |_| "clip");
    if let Some(k) = m.get_mut("kind") {
        tagged(k, plain);
    }
    each(m, "children", |c| {
        if let Some(m) = c.get_mut("motion") {
            motion(m);
        }
    });
}

fn transition(t: &mut Value) {
    if let Some(d) = t.get_mut("dest") {
        tagged(d, |_| "state");
    }
    each(t, "conditions", |c| {
        if let Some(m) = c.get_mut("mode") {
            unit(m);
        }
    });
    if let Some(i) = t.get_mut("interruption") {
        unit(i);
    }
}

/// A state behaviour: dialect 1's own variants, with native entry operations and pose parts inside.
fn behaviour(b: &mut Value) {
    if let Some(d) = b.get_mut("ParameterDriver") {
        each(d, "ops", |op| {
            tagged(op, plain);
            if let Some(r) = op.get_mut("remap")
                && let Some([from, to]) = r.as_array().and_then(|a| <&[Value; 2]>::try_from(a.as_slice()).ok())
            {
                *r = json!({"from": from, "to": to});
            }
        });
    }
    if let Some(parts) = b.get_mut("TrackingControl").and_then(Value::as_array_mut) {
        for p in parts.iter_mut().filter_map(Value::as_array_mut) {
            p.iter_mut().for_each(unit);
        }
    }
}

/// `X_animator.clips`: constraint bindings name native constraint types and fields.
pub(super) fn clips(clips: &mut Value) {
    for c in clips.as_array_mut().into_iter().flatten() {
        each(c, "curves", |curve| {
            if let Some(k) = curve.pointer_mut("/binding/property/Constraint") {
                if let Some(t) = k.get_mut("kind") {
                    unit(t);
                }
                if let Some(f) = k.get_mut("field") {
                    tagged(f, |_| "source");
                }
            }
        });
    }
}

/// `X_constraints`.
pub(super) fn constraints(v: &mut Value) {
    for c in v.as_array_mut().into_iter().flatten() {
        let Some(k) = c.get_mut("kind") else {
            continue;
        };
        tagged(k, plain);
        if let Some(up) = k.get_mut("world_up") {
            if let Some([node, vector]) = up
                .get("ObjectRotationUp")
                .and_then(Value::as_array)
                .and_then(|a| <&[Value; 2]>::try_from(a.as_slice()).ok())
            {
                *up = json!({"type": "object_rotation_up", "node": node, "up": vector});
            } else {
                tagged(up, |k| if k == "Vector" { "up" } else { "node" });
            }
        }
    }
}

/// `X_dynamics`.
pub(super) fn dynamics(v: &mut Value) {
    each(v, "chains", |c| {
        for k in ["multi_child", "version", "integration", "immobile_type"] {
            if let Some(x) = c.get_mut(k) {
                unit(x);
            }
        }
        if let Some(l) = c.get_mut("limit") {
            tagged(l, plain);
        }
    });
    each(v, "colliders", |c| {
        if let Some(s) = c.get_mut("shape") {
            tagged(s, plain);
        }
    });
}

/// `X_contacts`.
pub(super) fn contacts(v: &mut Value) {
    let shape = |c: &mut Value| {
        if let Some(s) = c.pointer_mut("/volume/shape") {
            tagged(s, plain);
        }
    };
    each(v, "senders", shape);
    each(v, "receivers", |r| {
        shape(r);
        if let Some(k) = r.get_mut("kind") {
            unit(k);
        }
    });
}
