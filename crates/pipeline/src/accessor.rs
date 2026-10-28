//! Bounded glTF accessor reading: every offset, stride, count and sparse index is checked before use.
//!
//! The `gltf` crate's own reader uses unchecked arithmetic on untrusted counts, so it is not used.

use gltf::accessor::{Accessor, DataType, Dimensions};

/// Reads `acc` as floats, `count * components` long, applying glTF normalisation for integer types.
pub fn floats(acc: &Accessor, buffers: &[Vec<u8>], max_count: usize) -> Result<Vec<f32>, String> {
    read(acc, buffers, max_count, |b, ty, normalized| {
        let raw = match ty {
            DataType::I8 => b[0] as i8 as f32,
            DataType::U8 => b[0] as f32,
            DataType::I16 => i16::from_le_bytes([b[0], b[1]]) as f32,
            DataType::U16 => u16::from_le_bytes([b[0], b[1]]) as f32,
            DataType::U32 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
            DataType::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        };
        match (normalized, ty) {
            (true, DataType::I8) => (raw / 127.0).max(-1.0),
            (true, DataType::U8) => raw / 255.0,
            (true, DataType::I16) => (raw / 32767.0).max(-1.0),
            (true, DataType::U16) => raw / 65535.0,
            _ => raw,
        }
    })
}

/// Reads `acc` as unsigned integers; only unsigned component types are accepted.
pub fn uints(acc: &Accessor, buffers: &[Vec<u8>], max_count: usize) -> Result<Vec<u32>, String> {
    if !matches!(acc.data_type(), DataType::U8 | DataType::U16 | DataType::U32) {
        return Err(format!("accessor {}: expected an unsigned integer type", acc.index()));
    }
    read(acc, buffers, max_count, |b, _, _| uint(b))
}

/// Reads `acc` as fixed-size float arrays, requiring exactly `N` components per element.
pub fn arrays<const N: usize>(acc: &Accessor, buffers: &[Vec<u8>], max_count: usize) -> Result<Vec<[f32; N]>, String> {
    if acc.dimensions().multiplicity() != N {
        return Err(format!("accessor {}: expected {N} components", acc.index()));
    }
    let flat = floats(acc, buffers, max_count)?;
    Ok(flat.as_chunks::<N>().0.to_vec())
}

fn uint(b: &[u8]) -> u32 {
    match b.len() {
        1 => b[0] as u32,
        2 => u16::from_le_bytes([b[0], b[1]]) as u32,
        _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

fn view_bytes<'b>(view: &gltf::buffer::View, buffers: &'b [Vec<u8>]) -> Result<&'b [u8], String> {
    let buffer = buffers
        .get(view.buffer().index())
        .ok_or_else(|| format!("bufferView {}: buffer missing", view.index()))?;
    let end = view.offset().checked_add(view.length());
    end.and_then(|end| buffer.get(view.offset()..end))
        .ok_or_else(|| format!("bufferView {}: out of buffer bounds", view.index()))
}

/// Copies `count` elements of `size` bytes, `stride` apart from `offset`, converting each component.
#[allow(clippy::too_many_arguments)]
fn elements<T>(
    data: &[u8],
    offset: usize,
    stride: usize,
    count: usize,
    components: usize,
    ty: DataType,
    normalized: bool,
    out: &mut Vec<T>,
    conv: &impl Fn(&[u8], DataType, bool) -> T,
) -> Result<(), String> {
    let size = ty.size();
    for i in 0..count {
        let at = i.checked_mul(stride).and_then(|x| x.checked_add(offset));
        let element = at
            .and_then(|at| data.get(at..at.checked_add(components * size)?))
            .ok_or("accessor out of bufferView bounds")?;
        out.extend(element.chunks_exact(size).map(|c| conv(c, ty, normalized)));
    }
    Ok(())
}

fn read<T: Copy + Default>(
    acc: &Accessor,
    buffers: &[Vec<u8>],
    max_count: usize,
    conv: impl Fn(&[u8], DataType, bool) -> T,
) -> Result<Vec<T>, String> {
    let err = |e: String| format!("accessor {}: {e}", acc.index());
    let (count, ty, normalized) = (acc.count(), acc.data_type(), acc.normalized());
    let components = acc.dimensions().multiplicity();
    // Small-component matrices carry column padding; nothing we import uses them.
    if matches!(acc.dimensions(), Dimensions::Mat2 | Dimensions::Mat3 | Dimensions::Mat4) && ty != DataType::F32 {
        return Err(err("non-float matrices are not supported".into()));
    }
    if count > max_count {
        return Err(err(format!("{count} elements exceeds limit {max_count}")));
    }
    let element = components * ty.size();
    let mut out = Vec::with_capacity(count * components);
    match acc.view() {
        Some(view) => {
            let stride = view.stride().unwrap_or(element);
            if stride < element {
                return Err(err("stride shorter than element".into()));
            }
            let data = view_bytes(&view, buffers).map_err(err)?;
            elements(
                data,
                acc.offset(),
                stride,
                count,
                components,
                ty,
                normalized,
                &mut out,
                &conv,
            )
            .map_err(err)?;
        }
        None => out.resize(count * components, T::default()),
    }
    if let Some(sparse) = acc.sparse() {
        let n = sparse.count();
        if n > count {
            return Err(err("more sparse values than elements".into()));
        }
        let (ix, vals) = (sparse.indices(), sparse.values());
        let ix_size = ix.index_type().size();
        let ix_data = view_bytes(&ix.view(), buffers).map_err(err)?;
        let mut indices = Vec::with_capacity(n);
        let mut values = Vec::with_capacity(n * components);
        // Sparse index and value arrays are tightly packed.
        elements(
            ix_data,
            ix.offset(),
            ix_size,
            n,
            1,
            ty_of_size(ix_size),
            false,
            &mut indices,
            &|b, _, _| uint(b),
        )
        .map_err(err)?;
        let val_data = view_bytes(&vals.view(), buffers).map_err(err)?;
        elements(
            val_data,
            vals.offset(),
            element,
            n,
            components,
            ty,
            normalized,
            &mut values,
            &conv,
        )
        .map_err(err)?;
        for (i, v) in indices.iter().zip(values.chunks_exact(components)) {
            let i = *i as usize;
            if i >= count {
                return Err(err(format!("sparse index {i} out of range")));
            }
            out[i * components..(i + 1) * components].copy_from_slice(v);
        }
    }
    Ok(out)
}

fn ty_of_size(size: usize) -> DataType {
    match size {
        1 => DataType::U8,
        2 => DataType::U16,
        _ => DataType::U32,
    }
}
