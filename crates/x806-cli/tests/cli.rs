use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use x806_core::codec::{dat, ini};
use x806_core::model::{Macro, MacroEvent, MacroEventKind, RepeatMode};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn x806_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_x806"))
}

#[test]
fn cli_udev_outputs_valid_rule() {
    let out = x806_cmd().arg("udev").output().expect("run x806 udev");
    assert!(
        out.status.success(),
        "x806 udev failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ATTRS{idVendor}==\"30fa\""),
        "missing ATTRS{{idVendor}} in stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("ATTRS{idProduct}==\"1340\""),
        "missing ATTRS{{idProduct}} in stdout:\n{stdout}"
    );
}

#[test]
fn cli_reset_dry_run_compiles_plan() {
    let out = x806_cmd()
        .args(["reset", "--dry-run"])
        .output()
        .expect("run x806 reset --dry-run");
    assert!(
        out.status.success(),
        "x806 reset --dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("EEPROM writes:") || stdout.contains("writes"),
        "expected write count in stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("macro bytes") || stdout.contains("Macro bytes"),
        "expected macro bytes used in stdout:\n{stdout}"
    );
}

#[test]
fn cli_import_export_and_apply_dry_run_roundtrip() {
    let tmp = TempDir::new("x806_cli_roundtrip");
    let src_dir = tmp.path.join("src_bundle");
    let macros_dir = src_dir.join("macro");
    std::fs::create_dir_all(&macros_dir).expect("create macros dir");

    // 1. Write config.ini, macroGrp.ini, and Streaming.dat (ZoomIt: LCtrl + '4')
    let ini_text = concat!(
        "[Init]\r\n",
        "VER_SEL=1\r\n",
        "CurConfig=0\r\n",
        "[KEYMAP]\r\n",
        "K1_MAP=7\r\n",
        "K2_MAP=3\r\n",
        "K3_MAP=2\r\n",
        "K4_MAP=1\r\n",
        "K5_MAP=9\r\n",
        "K6_MAP=6\r\n",
        "K7_MAP=5\r\n",
        "K8_MAP=4\r\n",
        "[Config0]\r\n",
        "K0=256\r\n",
        "K1=42,31\r\n",
        "[ShareData]\r\n",
        "USB_SPEED=2\r\n",
    );
    let config_ini = src_dir.join("config.ini");
    std::fs::write(&config_ini, ini_text).expect("write config.ini");

    let grp_bytes = ini::encode_macro_groups(&["Streaming".to_string()]);
    std::fs::write(macros_dir.join("macroGrp.ini"), grp_bytes).expect("write macroGrp.ini");

    let zoom_it = Macro {
        name: "ZoomIt".to_string(),
        repeat_mode: RepeatMode::Once,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0xE0 },
                delay_ms: 10,
            },
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x21 },
                delay_ms: 10,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: 0x21 },
                delay_ms: 10,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: 0xE0 },
                delay_ms: 10,
            },
        ],
        led_blocks: Vec::new(),
    };
    let dat_text = dat::encode_dat(&[zoom_it], false);
    std::fs::write(macros_dir.join("Streaming.dat"), dat_text).expect("write Streaming.dat");

    // 2. Import bundle to TOML
    let toml_path = tmp.path.join("x806.toml");
    let import_out = x806_cmd()
        .arg("import")
        .arg(&config_ini)
        .arg("--macros-dir")
        .arg(&macros_dir)
        .arg("--out")
        .arg(&toml_path)
        .output()
        .expect("run x806 import");
    assert!(
        import_out.status.success(),
        "x806 import failed: {}",
        String::from_utf8_lossy(&import_out.stderr)
    );
    assert!(toml_path.exists(), "imported TOML file was not created");

    // 3. Apply --dry-run from TOML
    let apply_out = x806_cmd()
        .arg("apply")
        .arg(&toml_path)
        .arg("--dry-run")
        .output()
        .expect("run x806 apply --dry-run");
    assert!(
        apply_out.status.success(),
        "x806 apply --dry-run failed: {}",
        String::from_utf8_lossy(&apply_out.stderr)
    );
    let apply_stdout = String::from_utf8_lossy(&apply_out.stdout);
    assert!(
        apply_stdout.contains("dry-run") || apply_stdout.contains("Dry-run"),
        "expected dry-run indicator in stdout:\n{apply_stdout}"
    );

    // 4. Export TOML back to config.ini + macro dir
    let export_dir = tmp.path.join("exported_bundle");
    let export_ini = export_dir.join("config.ini");
    let export_macros = export_dir.join("macro");
    let export_out = x806_cmd()
        .arg("export")
        .arg(&toml_path)
        .arg("--out")
        .arg(&export_ini)
        .arg("--macros-dir")
        .arg(&export_macros)
        .output()
        .expect("run x806 export");
    assert!(
        export_out.status.success(),
        "x806 export failed: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );
    let exported_dat = export_macros.join("Streaming.dat");
    assert!(exported_dat.exists(), "exported Streaming.dat missing");

    // 5. Inspect exported .dat
    let inspect_out = x806_cmd()
        .args(["macro", "inspect"])
        .arg(&exported_dat)
        .output()
        .expect("run x806 macro inspect");
    assert!(
        inspect_out.status.success(),
        "x806 macro inspect failed: {}",
        String::from_utf8_lossy(&inspect_out.stderr)
    );
    let inspect_stdout = String::from_utf8_lossy(&inspect_out.stdout);
    assert!(
        inspect_stdout.contains("ZoomIt"),
        "expected 'ZoomIt' in inspect output:\n{inspect_stdout}"
    );
    assert!(
        inspect_stdout.contains("LCONTROL"),
        "expected 'LCONTROL' in inspect output:\n{inspect_stdout}"
    );
}
