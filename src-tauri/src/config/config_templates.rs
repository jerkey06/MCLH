use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::collections::HashMap;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::error::{AppError, Result};

pub fn apply_template(
    template_name: &str,
    replacements: &HashMap<String, String>,
    output_path: &Path,
    state: Arc<AppState>
) -> Result<()> {
    let server_dir = &state.server_directory;
    let templates_dir = Path::new(server_dir).join("templates");
    let template_path = templates_dir.join(template_name);

    if !template_path.exists() {
        return Err(AppError::ConfigError(
            format!("Template file {} not found", template_name)
        ));
    }

    let content = fs::read_to_string(&template_path)?;
    let mut result = content.clone();

    for (key, value) in replacements {
        let placeholder = format!("{{{{ {} }}}}", key);
        result = result.replace(&placeholder, value);
    }

    let parent_dir = output_path.parent().unwrap();
    if !parent_dir.exists() {
        fs::create_dir_all(parent_dir)?;
    }

    let mut file = File::create(output_path)?;
    file.write_all(result.as_bytes())?;

    Ok(())
}

pub fn install_default_templates(state: Arc<AppState>) -> Result<()> {
    let server_dir = &state.server_directory;
    let templates_dir = Path::new(server_dir).join("templates");

    if !templates_dir.exists() {
        fs::create_dir_all(&templates_dir)?;
    }

    // Server properties template
    let server_properties_template = r#"#Minecraft server properties
#{{ timestamp }}
server-port={{ port }}
gamemode={{ gamemode }}
difficulty={{ difficulty }}
level-seed={{ seed }}
enable-command-block={{ command_blocks }}
max-players={{ max_players }}
spawn-protection={{ spawn_protection }}
view-distance={{ view_distance }}
spawn-npcs={{ spawn_npcs }}
spawn-animals={{ spawn_animals }}
spawn-monsters={{ spawn_monsters }}
pvp={{ pvp }}
"#;

    let template_path = templates_dir.join("server.properties.tmpl");
    if !template_path.exists() {
        let mut file = File::create(template_path)?;
        file.write_all(server_properties_template.as_bytes())?;
    }

    // spigot.yml template
    let spigot_template = r#"settings:
  bungeecord: false
  restart-on-crash: true
  restart-script-location: ./start.sh
  sample-count: 12
  timeout-time: 60
  player-shuffle: 0
  user-cache-size: 1000
  save-user-cache-on-stop-only: false
messages:
  restart: Server is restarting
  whitelist: You are not whitelisted on this server!
  unknown-command: Unknown command. Type "/help" for help.
  server-full: The server is full!
  outdated-client: Outdated client! Please use {{ minecraft_version }}
  outdated-server: Outdated server! I'm still on {{ minecraft_version }}
advancements:
  disable-saving: false
  disabled:
  - minecraft:story/disabled_advancement
stats:
  disable-saving: false
  forced-stats: {}
commands:
  spam-exclusions:
  - /skill
  silent-commandblock-console: false
  replace-commands:
  - setblock
  - summon
  - testforblock
  - tellraw
  log: true
  tab-complete: 0
  send-namespaced: true
players:
  disable-saving: false
world-settings:
  default:
    verbose: false
    mob-spawn-range: 6
    growth:
      cactus-modifier: 100
      cane-modifier: 100
      melon-modifier: 100
      mushroom-modifier: 100
      pumpkin-modifier: 100
      sapling-modifier: 100
      beetroot-modifier: 100
      carrot-modifier: 100
      potato-modifier: 100
      wheat-modifier: 100
      netherwart-modifier: 100
      vine-modifier: 100
      cocoa-modifier: 100
    entity-activation-range:
      animals: 32
      monsters: 32
      raiders: 48
      misc: 16
      water: 16
      villagers: 32
      flying-monsters: 32
    entity-tracking-range:
      players: 48
      animals: 48
      monsters: 48
      misc: 32
      other: 64
    ticks-per:
      hopper-transfer: 8
      hopper-check: 1
    hopper-amount: 1
    merge-radius:
      item: 2.5
      exp: 3.0
    item-despawn-rate: 6000
    view-distance: {{ view_distance }}
    enable-zombie-pigmen-portal-spawns: true
    wither-spawn-sound-radius: 0
    arrow-despawn-rate: 1200
    zombie-aggressive-towards-villager: true
    nerf-spawner-mobs: false
"#;

    let template_path = templates_dir.join("spigot.yml.tmpl");
    if !template_path.exists() {
        let mut file = File::create(template_path)?;
        file.write_all(spigot_template.as_bytes())?;
    }

    // bukkit.yml template
    let bukkit_template = r#"settings:
  allow-end: true
  warn-on-overload: true
  permissions-file: permissions.yml
  update-folder: update
  plugin-profiling: false
  connection-throttle: 4000
  query-plugins: true
  deprecated-verbose: default
  shutdown-message: Server closed
  minimum-api: none
spawn-limits:
  monsters: 70
  animals: 10
  water-animals: 5
  water-ambient: 20
  water-underground-creature: 5
  ambient: 15
chunk-gc:
  period-in-ticks: 600
ticks-per:
  animal-spawns: 400
  monster-spawns: 1
  water-spawns: 1
  water-ambient-spawns: 1
  water-underground-creature-spawns: 1
  ambient-spawns: 1
  autosave: 6000
aliases: now-in-commands.yml
"#;

    let template_path = templates_dir.join("bukkit.yml.tmpl");
    if !template_path.exists() {
        let mut file = File::create(template_path)?;
        file.write_all(bukkit_template.as_bytes())?;
    }

    Ok(())
}