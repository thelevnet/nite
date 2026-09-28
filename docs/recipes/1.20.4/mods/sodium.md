# sodium Configuration (1.20.4)

**Target File:** `config/sodium-options.json`

Configure under `[instance.mods_config.sodium]`.

## Settings Map

| nite Setting | Target Key | Expected Type |
|---|---|---|
| `chunk_builder_threads` | `performance.chunk_builder_threads` | `int` |
| `chunk_build_defer_mode` | `performance.chunk_build_defer_mode` | `string` |
| `animate_only_visible_textures` | `performance.animate_only_visible_textures` | `bool` |
| `use_entity_culling` | `performance.use_entity_culling` | `bool` |
| `use_fog_occlusion` | `performance.use_fog_occlusion` | `bool` |
| `use_block_face_culling` | `performance.use_block_face_culling` | `bool` |
| `use_no_error_gl_context` | `performance.use_no_error_g_l_context` | `bool` |
| `cpu_render_ahead_limit` | `advanced.cpu_render_ahead_limit` | `int` |
| `use_advanced_staging_buffers` | `advanced.use_advanced_staging_buffers` | `bool` |
| `hidden_fluid_culling` | `quality.hidden_fluid_culling` | `bool` |
| `improved_fluid_shaping` | `quality.improved_fluid_shaping` | `bool` |
| `pixel_filtering_mode` | `quality.pixel_filtering_mode` | `string` |
