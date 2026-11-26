//! glTF materials and textures: metallic-roughness and unlit materials, texture slots and their images.

use civbb_format::BlobId;
use civbb_format::material::{
    AlphaMode, ColorSpace, ImageFormat, Material, MaterialModel, Pbr, Texture, TextureSlot, Unlit,
};

use super::Importer;

impl Importer<'_> {
    pub(super) fn material(&mut self, m: &gltf::Material) -> Material {
        let pbr = m.pbr_metallic_roughness();
        let alpha = match m.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask {
                cutoff: m.alpha_cutoff().unwrap_or(0.5),
            },
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        };
        let base_tex = pbr
            .base_color_texture()
            .and_then(|t| self.info_slot(&t, ColorSpace::Srgb));
        let model = if m.unlit() {
            MaterialModel::Unlit(Unlit {
                color: pbr.base_color_factor(),
                color_tex: base_tex,
            })
        } else {
            let normal = m.normal_texture();
            let occlusion = m.occlusion_texture();
            MaterialModel::Pbr(Pbr {
                base_color: pbr.base_color_factor(),
                base_color_tex: base_tex,
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                metallic_roughness_tex: pbr
                    .metallic_roughness_texture()
                    .and_then(|t| self.info_slot(&t, ColorSpace::Linear)),
                normal_tex: normal
                    .as_ref()
                    .and_then(|t| self.slot(&t.texture(), t.tex_coord(), None, ColorSpace::Linear)),
                normal_scale: normal.as_ref().map_or(1.0, |t| t.scale()),
                occlusion_tex: occlusion
                    .as_ref()
                    .and_then(|t| self.slot(&t.texture(), t.tex_coord(), None, ColorSpace::Linear)),
                emissive: m.emissive_factor(),
                emissive_tex: m.emissive_texture().and_then(|t| self.info_slot(&t, ColorSpace::Srgb)),
            })
        };
        Material {
            name: m.name().unwrap_or("material").into(),
            model,
            alpha,
            double_sided: m.double_sided(),
            order: 0,
        }
    }

    fn info_slot(&mut self, info: &gltf::texture::Info, space: ColorSpace) -> Option<TextureSlot> {
        let transform = info.texture_transform();
        let tex_coord = transform
            .as_ref()
            .and_then(|t| t.tex_coord())
            .unwrap_or(info.tex_coord());
        if transform.as_ref().is_some_and(|t| t.rotation() != 0.0) {
            self.warnings
                .push("KHR_texture_transform rotation is not supported; ignored".into());
        }
        let scale_offset = transform.map(|t| {
            let (s, o) = (t.scale(), t.offset());
            [s[0], s[1], o[0], o[1]]
        });
        self.slot(&info.texture(), tex_coord, scale_offset, space)
    }

    pub(super) fn slot(
        &mut self,
        texture: &gltf::Texture,
        tex_coord: u32,
        scale_offset: Option<[f32; 4]>,
        space: ColorSpace,
    ) -> Option<TextureSlot> {
        if tex_coord > 1 {
            self.warnings.push(format!(
                "texture {}: TEXCOORD_{tex_coord} unsupported; slot dropped",
                texture.index()
            ));
            return None;
        }
        let image = texture.source().index();
        let key = (image, space == ColorSpace::Srgb);
        let index = match self.texture_map.get(&key) {
            Some(i) => *i,
            None => {
                let name = texture.name().unwrap_or("texture");
                let i = match self.image(image, name, space) {
                    Ok(i) => Some(i),
                    Err(e) => {
                        self.warnings.push(format!("image {image}: {e}; slot dropped"));
                        None
                    }
                };
                self.texture_map.insert(key, i);
                i
            }
        }?;
        Some(TextureSlot {
            texture: index,
            uv_set: tex_coord as u8,
            scale_offset: scale_offset.unwrap_or([1.0, 1.0, 0.0, 0.0]),
        })
    }

    /// An image's encoded bytes and format. Read from the JSON: the wrapper panics on malformed images.
    pub(super) fn image_bytes(&mut self, image: usize) -> Result<(Vec<u8>, ImageFormat), String> {
        let doc = self.doc;
        let json = &doc.as_json().images[image];
        let bytes = match (&json.buffer_view, &json.uri) {
            (Some(view), _) => {
                let view = doc.views().nth(view.value()).expect("validated index");
                let buffer = &self.buffers[view.buffer().index()];
                let end = view.offset().checked_add(view.length());
                end.and_then(|end| buffer.get(view.offset()..end))
                    .ok_or("bufferView out of bounds")?
                    .to_vec()
            }
            (None, Some(uri)) => self.load_uri(uri)?,
            (None, None) => return Err("no source".into()),
        };
        let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ImageFormat::Png
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            ImageFormat::Jpeg
        } else {
            return Err("not PNG or JPEG".into());
        };
        Ok((bytes, format))
    }

    /// Stores an image's encoded bytes as a blob.
    fn image(&mut self, image: usize, name: &str, space: ColorSpace) -> Result<u32, String> {
        let (bytes, format) = self.image_bytes(image)?;
        let json = &self.doc.as_json().images[image];
        let blob = BlobId::of(&bytes);
        self.blobs.insert(blob, bytes);
        self.textures.push(Texture {
            name: json.name.clone().unwrap_or_else(|| name.into()),
            format,
            color_space: space,
            blob,
        });
        Ok(self.textures.len() as u32 - 1)
    }

    pub(super) fn primitive_material(&mut self, p: &gltf::Primitive) -> u32 {
        if let Some(i) = p.material().index() {
            return i as u32;
        }
        *self.default_material.get_or_insert_with(|| {
            self.materials.push(Material {
                name: "default".into(),
                model: MaterialModel::Pbr(Pbr {
                    base_color: [1.0; 4],
                    base_color_tex: None,
                    metallic: 1.0,
                    roughness: 1.0,
                    metallic_roughness_tex: None,
                    normal_tex: None,
                    normal_scale: 1.0,
                    occlusion_tex: None,
                    emissive: [0.0; 3],
                    emissive_tex: None,
                }),
                alpha: AlphaMode::Opaque,
                double_sided: false,
                order: 0,
            });
            self.materials.len() as u32 - 1
        })
    }
}
