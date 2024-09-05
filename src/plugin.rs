use bevy::{
    asset::load_internal_asset,
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::{
            CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
            ImageSamplerDescriptor, ImageType,
        },
    },
};

use crate::{
    configuration::{DefaultWorld, VoxelWorldConfig},
    voxel_material::{
        prepare_texture, LoadingTexture, StandardVoxelMaterial, TextureLayers,
        VOXEL_TEXTURE_SHADER_HANDLE,
    },
    voxel_world::*,
    voxel_world_internal::Internals,
};

#[derive(Resource)]
pub struct VoxelWorldMaterialHandle<M: Material> {
    pub handle: Handle<M>,
}

/// The main plugin for the voxel world. This plugin sets up the voxel world and its dependencies.
/// The type parameter `C` is used to differentiate between different voxel worlds with different configs.
pub struct VoxelWorldPlugin<C, M = StandardMaterial>
where
    C: VoxelWorldConfig,
    M: Material,
{
    spawn_meshes: bool,
    use_custom_material: bool,
    config: C,
    material: M,
}

impl<C> VoxelWorldPlugin<C, StandardMaterial>
where
    C: VoxelWorldConfig,
{
    pub fn with_config(config: C) -> Self {
        Self {
            config,
            spawn_meshes: true,
            use_custom_material: false,
            material: StandardMaterial::default(),
        }
    }

    pub fn minimal() -> Self
    where
        C: Default,
    {
        Self {
            spawn_meshes: false,
            use_custom_material: false,
            config: C::default(),
            material: StandardMaterial::default(),
        }
    }
}

impl<C, M> VoxelWorldPlugin<C, M>
where
    C: VoxelWorldConfig,
    M: Material,
{
    /// Use this to tell `bevy_voxel_world` to use a custom material. This can be any material that
    /// implements `bevy::pbr::Material`. You can use this to create custom shaders for your voxel
    /// world. You can set this up like any other material in Bevy.
    ///
    /// `bevy_voxel_world` will add the material as an asset, so you can query for it later using
    /// `Res<Assets<MyCustomVoxelMaterialType>>`.
    pub fn with_material<CustomMaterial: Material>(
        self,
        material: CustomMaterial,
    ) -> VoxelWorldPlugin<C, CustomMaterial> {
        VoxelWorldPlugin {
            spawn_meshes: self.spawn_meshes,
            use_custom_material: true,
            config: self.config,
            material,
        }
    }
}

impl Default for VoxelWorldPlugin<DefaultWorld, StandardMaterial> {
    fn default() -> Self {
        Self {
            spawn_meshes: true,
            use_custom_material: false,
            config: DefaultWorld,
            material: StandardMaterial::default(),
        }
    }
}

impl<C, M> Plugin for VoxelWorldPlugin<C, M>
where
    C: VoxelWorldConfig,
    M: Material,
{
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .add_systems(PreStartup, Internals::<C>::setup)
            .add_systems(
                PreUpdate,
                (
                    (
                        (Internals::<C>::spawn_chunks, Internals::<C>::retire_chunks).chain(),
                        Internals::<C>::remesh_dirty_chunks,
                    )
                        .chain(),
                    (
                        Internals::<C>::flush_voxel_write_buffer,
                        Internals::<C>::despawn_retired_chunks,
                        (
                            Internals::<C>::flush_chunk_map_buffers,
                            Internals::<C>::flush_mesh_cache_buffers,
                        ),
                    )
                        .chain(),
                ),
            )
            .add_event::<ChunkWillSpawn<C>>()
            .add_event::<ChunkWillDespawn<C>>()
            .add_event::<ChunkWillRemesh<C>>();

        // Spawning of meshes is optional, mainly to simplify testing.
        // This makes voxel_world work with a MinimalPlugins setup.
        if self.spawn_meshes {
            load_internal_asset!(
                app,
                VOXEL_TEXTURE_SHADER_HANDLE,
                "shaders/voxel_texture.wgsl",
                Shader::from_wgsl
            );

            app.add_systems(Update, Internals::<C>::spawn_meshes);
        }

        if !self.use_custom_material && self.spawn_meshes {
            let mat_plugins = app.get_added_plugins::<MaterialPlugin::<
                ExtendedMaterial<StandardMaterial, StandardVoxelMaterial>>>();

            if mat_plugins.is_empty() {
                app.add_plugins(MaterialPlugin::<
                    ExtendedMaterial<StandardMaterial, StandardVoxelMaterial>,
                >::default());
            }

            let mut preloaded_texture = true;
            let mut preloaded_normal = true;
            let texture_conf = self.config.voxel_texture();
            let mut texture_layers = 0;

            // Use built-in default texture if no texture is specified.
            let (image_handle, normal_handle) = if texture_conf.is_none() {
                let mut image = Image::from_buffer(
                    include_bytes!("shaders/default_texture.png"),
                    ImageType::MimeType("image/png"),
                    CompressedImageFormats::default(),
                    false,
                    ImageSampler::Default,
                    RenderAssetUsages::default(),
                )
                .unwrap();
                image.reinterpret_stacked_2d_as_array(4);
                let mut image_assets = app.world_mut().resource_mut::<Assets<Image>>();
                let normal_image = default_normal_map(4);
                (image_assets.add(image), image_assets.add(normal_image))
            } else {
                let texture = texture_conf.unwrap();
                texture_layers = texture.index_count;
                let asset_server = app.world().get_resource::<AssetServer>().unwrap();
                preloaded_texture = false;

                let image = asset_server.load(texture.path);

                let normal_image = if let Some(normal_path) = texture.normal_path {
                    preloaded_normal = false;
                    asset_server.load(normal_path)
                } else {
                    let mut image_assets = app.world_mut().resource_mut::<Assets<Image>>();
                    let normal = default_normal_map(texture_layers);
                    image_assets.add(normal)
                };

                (image, normal_image)
            };

            let mut material_assets = app
                .world_mut()
                .resource_mut::<Assets<ExtendedMaterial<StandardMaterial, StandardVoxelMaterial>>>(
                );

            let mat_handle = material_assets.add(ExtendedMaterial {
                base: StandardMaterial {
                    reflectance: 0.05,
                    metallic: 0.05,
                    perceptual_roughness: 0.95,
                    ..default()
                },
                extension: StandardVoxelMaterial {
                    voxels_texture: image_handle.clone(),
                    normal_texture: normal_handle.clone(),
                },
            });

            app.insert_resource(LoadingTexture {
                is_loaded: preloaded_texture,
                is_loaded_normal: preloaded_normal,
                handle: image_handle,
                normal_handle: normal_handle.clone(),
            });
            app.insert_resource(VoxelWorldMaterialHandle { handle: mat_handle });
            app.insert_resource(TextureLayers(texture_layers));

            app.add_systems(Update, prepare_texture);

            app.add_systems(
                Update,
                Internals::<C>::assign_material::<
                    ExtendedMaterial<StandardMaterial, StandardVoxelMaterial>,
                >,
            );
        }

        if self.use_custom_material {
            if self.config.init_custom_materials() {
                let mut custom_material_assets = app.world_mut().resource_mut::<Assets<M>>();
                let handle = custom_material_assets.add(self.material.clone());
                app.insert_resource(VoxelWorldMaterialHandle { handle });
            }

            app.insert_resource(LoadingTexture {
                is_loaded: true,
                is_loaded_normal: true,
                handle: Handle::default(),
                normal_handle: Handle::default(),
            });

            app.add_systems(Update, Internals::<C>::assign_material::<M>);
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
