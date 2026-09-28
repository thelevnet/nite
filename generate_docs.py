import os
import toml
import glob

def generate_mod_docs():
    recipes_dir = 'recipes'
    docs_dir = 'docs/recipes'

    for version_dir in os.listdir(recipes_dir):
        v_path = os.path.join(recipes_dir, version_dir)
        if not os.path.isdir(v_path):
            continue

        out_v_dir = os.path.join(docs_dir, version_dir)
        os.makedirs(out_v_dir, exist_ok=True)
        
        # Options
        opt_path = os.path.join(v_path, 'options.toml')
        if os.path.exists(opt_path):
            with open(opt_path, 'r') as f:
                opt_data = toml.load(f)
            
            with open(os.path.join(out_v_dir, 'options.md'), 'w') as f:
                f.write(f"# Vanilla Options ({version_dir})\n\n")
                f.write("This maps settings in `[instance.settings]` to vanilla `options.txt`.\n\n")
                f.write("| nite Setting | Target Key | Expected Type |\n")
                f.write("|---|---|---|\n")
                for key, val in opt_data.get('settings_map', {}).items():
                    f.write(f"| `{key}` | `{val['target']}` | `{val['type']}` |\n")

        # Mods
        mods_path = os.path.join(v_path, 'mods')
        out_mods_dir = os.path.join(out_v_dir, 'mods')
        os.makedirs(out_mods_dir, exist_ok=True)
        if os.path.exists(mods_path):
            for mod_file in os.listdir(mods_path):
                if not mod_file.endswith('.toml'):
                    continue
                mod_name = mod_file[:-5]
                with open(os.path.join(mods_path, mod_file), 'r') as f:
                    mod_data = toml.load(f)
                
                with open(os.path.join(out_mods_dir, f"{mod_name}.md"), 'w') as f:
                    f.write(f"# {mod_name} Configuration ({version_dir})\n\n")
                    f.write(f"**Target File:** `{mod_data.get('config_file', 'unknown')}`\n\n")
                    f.write(f"Configure under `[instance.mods_config.{mod_name}]`.\n\n")
                    
                    if 'settings_map' in mod_data:
                        f.write("## Settings Map\n\n")
                        f.write("| nite Setting | Target Key | Expected Type |\n")
                        f.write("|---|---|---|\n")
                        for key, val in mod_data['settings_map'].items():
                            f.write(f"| `{key}` | `{val['target']}` | `{val['type']}` |\n")
                    
                    if 'keybinds' in mod_data:
                        f.write("\n## Keybinds\n\n")
                        f.write("Keybinds declared here are safely mapped to `options.txt`.\n\n")
                        f.write("| nite Setting | Target Key |\n")
                        f.write("|---|---|\n")
                        for key, target in mod_data['keybinds'].items():
                            f.write(f"| `{key}` | `{target}` |\n")

def generate_index():
    with open('docs/README.md', 'w') as f:
        f.write("# nite Documentation\n\n")
        f.write("Welcome to the complete `nite` documentation. This folder contains the schema references for `instances.toml`, the Home Manager module, and configuration recipes for every supported version and mod.\n\n")
        f.write("## Table of Contents\n\n")
        f.write("- [Configuration Schema (instances.toml)](instances.md)\n")
        f.write("- [Home Manager Module Reference](home-manager.md)\n")
        f.write("- Recipes by Version:\n")
        
        versions = sorted([v for v in os.listdir('recipes') if os.path.isdir(os.path.join('recipes', v))])
        for v in versions:
            f.write(f"  - **{v}**\n")
            f.write(f"    - [Vanilla Options](recipes/{v}/options.md)\n")
            mods = sorted([m[:-5] for m in os.listdir(os.path.join('recipes', v, 'mods')) if m.endswith('.toml')])
            for m in mods:
                f.write(f"    - [{m}](recipes/{v}/mods/{m}.md)\n")
        
generate_mod_docs()
generate_index()
