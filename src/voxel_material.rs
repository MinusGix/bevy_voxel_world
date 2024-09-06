use bevy::{
    pbr::{MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    reflect::TypePath,
    render::{
        mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef, VertexAttributeDescriptor},
        render_asset::RenderAssetUsages,
        render_resource::{
            AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderRef,
            SpecializedMeshPipelineError, TextureDimension, TextureFormat, VertexFormat,
        },
        texture::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    },
};

/// Keeps track of the loading status of the image used for the voxel texture
#[derive(Resource)]
pub(crate) struct LoadingTexture {
    pub is_loaded: bool,
    pub is_loaded_normal: bool,
    pub texture_layers: Option<u32>,
    pub handle: Handle<Image>,
    pub normal_handle: Handle<Image>,
}

pub const VOXEL_TEXTURE_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(6998301138411443008);

pub(crate) const ATTRIBUTE_TEX_INDEX: MeshVertexAttribute =
    MeshVertexAttribute::new("TextureIndex", 989640910, VertexFormat::Uint32x3);

pub fn vertex_layout() -> Vec<VertexAttributeDescriptor> {
    vec![
        Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
        Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
        Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
        //Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
        Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
        Mesh::ATTRIBUTE_COLOR.at_shader_location(7),
        //Mesh::ATTRIBUTE_JOINT_INDEX.at_shader_location(6),
        //Mesh::ATTRIBUTE_JOINT_WEIGHT.at_shader_location(7),
        ATTRIBUTE_TEX_INDEX.at_shader_location(8),
    ]
}
#[derive(Asset, AsBindGroup, Debug, Clone, TypePath)]
pub(crate) struct StandardVoxelMaterial {
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    pub voxels_texture: Handle<Image>,
    #[texture(102, dimension = "2d_array")]
    #[sampler(103)]
    pub normal_texture: Handle<Image>,
}

impl MaterialExtension for StandardVoxelMaterial {
    fn fragment_shader() -> ShaderRef {
        VOXEL_TEXTURE_SHADER_HANDLE.into()
    }

    fn vertex_shader() -> ShaderRef {
        VOXEL_TEXTURE_SHADER_HANDLE.into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&vertex_layout())?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

pub(crate) fn prepare_texture(
    asset_server: Res<AssetServer>,
    mut loading_texture: ResMut<LoadingTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    // TODO: after initialization this system should never really run again. Can we kill it?
    let image_load_state = asset_server.get_load_state(loading_texture.handle.clone().id());
    let normal_image_load_state =
        asset_server.get_load_state(loading_texture.normal_handle.clone().id());
    if (loading_texture.is_loaded && loading_texture.is_loaded_normal)
        || image_load_state != Some(bevy::asset::LoadState::Loaded)
        || (!loading_texture.is_loaded_normal
            && normal_image_load_state != Some(bevy::asset::LoadState::Loaded))
    {
        return;
    }
    loading_texture.is_loaded = true;
    loading_texture.is_loaded_normal = true;

    let image = images.get_mut(&loading_texture.handle).unwrap();
    let texture_layers = loading_texture
        .texture_layers
        .unwrap_or_else(|| image.texture_descriptor.size.depth_or_array_layers);

    if image.texture_descriptor.size.depth_or_array_layers != texture_layers {
        image.reinterpret_stacked_2d_as_array(texture_layers);
    }

    let normal_image = images.get_mut(&loading_texture.normal_handle).unwrap();
    if loading_texture.is_loaded_normal {
        // TODO: is there a way to get around needing to know the number of layers here to ensure that the normal map in the shader
        // is a 2D array? We could use ifdefs for this... and then we don't have to create a normal map at all.
        // We put off creating the default normal map until now so that we know the number of layers
        *normal_image = default_normal_map(texture_layers);
    } else {
        if normal_image.texture_descriptor.size.depth_or_array_layers != texture_layers {
            normal_image.reinterpret_stacked_2d_as_array(texture_layers);
        }
    }
}

fn default_normal_map(layers: u32) -> Image {
    let size = Extent3d {
        width: 2,
        height: 2 * layers,       // Multiply height by layers
        depth_or_array_layers: 1, // Initially set to 1
    };

    // Create data for all layers
    let mut data = Vec::with_capacity((4 * 4 * layers) as usize);
    for _ in 0..layers {
        data.extend_from_slice(&[
            128, 128, 255, 255, 128, 128, 255, 255, 128, 128, 255, 255, 128, 128, 255, 255,
        ]);
    }

    let mut image = Image::new(
        size,
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::all(),
    );

    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..Default::default()
    });

    // Now this call will succeed
    image.reinterpret_stacked_2d_as_array(layers);

    image
}
