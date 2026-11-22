use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use directories::ProjectDirs;
use x806_core::codec::{dat, ini, toml as toml_codec};
use x806_core::eeprom::{CompileContext, EepromPlan, compile_plan};
use x806_core::hid::{HidApiTransport, X806Device};
use x806_core::keycode::hid_display_name;
use x806_core::model::{DeviceConfig, MacroEventKind, RepeatMode};
use x806_core::{PID, VID, X806Error, defaults};

const UDEV_RULES: &str = include_str!("../../../udev/99-x806.rules");
const EEPROM_RMW_PROFILE_ADDR: u16 = 0x0076;

#[derive(Parser)]
#[command(
    name = "x806",
    version,
    about = "Cross-platform CLI configurator and macro tool for the X806 / K806 8-key keypad"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Probe the attached X806 keypad and print firmware status (read-only)
    Status,
    /// Compile and flash a TOML or config.ini configuration to the keypad
    Apply(ApplyArgs),
    /// Restore factory-default keypad configuration
    Reset(ResetArgs),
    /// Import a Windows X806.exe bundle (config.ini + .dat) into TOML
    Import(ImportArgs),
    /// Export a TOML configuration back to X806.exe config.ini + .dat files
    Export(ExportArgs),
    /// Macro file utilities
    Macro(MacroArgs),
    /// Print Linux udev rules and installation instructions
    Udev,
}

#[derive(Args)]
struct ApplyArgs {
    /// Path to a .toml config or Windows config.ini (defaults to OS config directory)
    path: Option<PathBuf>,
    /// Override macro group directory when applying a .ini bundle
    #[arg(long)]
    macros_dir: Option<PathBuf>,
    /// Compile and validate the EEPROM write plan without touching hardware
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct ResetArgs {
    /// Compile and validate the factory-default EEPROM plan without touching hardware
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct ImportArgs {
    /// Path to Windows X806.exe config.ini
    config_ini: PathBuf,
    /// Path to macro directory containing macroGrp.ini and <group>.dat
    #[arg(long)]
    macros_dir: Option<PathBuf>,
    /// Path to PUBG_EN.dat or PUBG_CN.dat
    #[arg(long)]
    pubg_dat: Option<PathBuf>,
    /// Path to constMacro.dat
    #[arg(long)]
    const_macro: Option<PathBuf>,
    /// Write resulting TOML to file instead of stdout
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct ExportArgs {
    /// Path to input TOML configuration
    config_toml: PathBuf,
    /// Destination path for config.ini
    #[arg(long)]
    out: PathBuf,
    /// Destination directory for macroGrp.ini and <group>.dat (defaults to <out_dir>/macro)
    #[arg(long)]
    macros_dir: Option<PathBuf>,
}

#[derive(Args)]
struct MacroArgs {
    #[command(subcommand)]
    command: MacroCommands,
}

#[derive(Subcommand)]
enum MacroCommands {
    /// Parse a .dat macro library and print its macros, repeat modes, and events
    Inspect {
        /// Path to the .dat file
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Status => cmd_status(),
        Commands::Apply(args) => cmd_apply(args),
        Commands::Reset(args) => cmd_reset(args),
        Commands::Import(args) => cmd_import(args),
        Commands::Export(args) => cmd_export(args),
        Commands::Macro(args) => match args.command {
            MacroCommands::Inspect { file } => cmd_macro_inspect(&file),
        },
        Commands::Udev => {
            cmd_udev();
            Ok(())
        }
    }
}

fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    let (transport, manufacturer, product) = HidApiTransport::open_first()?;
    let mut dev = X806Device::with_strings(transport, manufacturer, product);

    let status = dev.read_status()?;
    let byte_0x76 = dev.read_eeprom_byte(EEPROM_RMW_PROFILE_ADDR)?;

    let eeprom_label = match status.macro_limit {
        0x0FF8 => "4 KB EEPROM (0x0FF8)".to_string(),
        0x07F8 => "2 KB EEPROM (0x07F8)".to_string(),
        other => format!("Custom EEPROM ({other:#06X})"),
    };
    let usable_macro_bytes = status.macro_limit.saturating_sub(0x0400);

    println!("Device:         VID {VID:#06X} / PID {PID:#06X}");
    println!(
        "Manufacturer:   {}",
        status.manufacturer.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "Product:        {}",
        status.product.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "Active profile: {} ({})",
        status.active_profile,
        profile_label(status.active_profile)
    );
    println!("Macro capacity: {eeprom_label} ({usable_macro_bytes} bytes usable)");
    println!("HW flags:       {:#04X}", status.hw_flags);
    println!("Raw report:     {:02X?}", status.raw_report);
    println!("EEPROM[0x0076]: {byte_0x76:#04X}");

    Ok(())
}

fn cmd_apply(args: ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let path = match args.path {
        Some(p) => p,
        None => default_config_path()?,
    };
    let cfg = load_config_auto(&path, args.macros_dir.as_deref())?;
    let label = format!("config '{}'", path.display());
    execute_plan_or_flash(&cfg, &label, args.dry_run)
}

fn cmd_reset(args: ResetArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = defaults::factory_default_config();
    execute_plan_or_flash(&cfg, "factory defaults", args.dry_run)
}

fn execute_plan_or_flash(
    cfg: &DeviceConfig,
    label: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run {
        let plan = compile_plan(cfg, CompileContext::default())?;
        print_plan_summary("Dry-run", label, cfg, &plan);
        println!(
            "Note: offline dry-run assumes the 4 KB EEPROM limit (0x0FF8 / 3064 usable bytes); 2 KB devices cap at 0x07F8 (1016 bytes)."
        );
        return Ok(());
    }

    let (transport, manufacturer, product) = HidApiTransport::open_first()?;
    let mut dev = X806Device::with_strings(transport, manufacturer, product);
    let plan = dev.flash_config(cfg)?;
    dev.send_exit()?;
    print_plan_summary("Flashed", label, cfg, &plan);
    Ok(())
}

fn print_plan_summary(mode: &str, label: &str, cfg: &DeviceConfig, plan: &EepromPlan) {
    let total_bytes: usize = plan.writes.iter().map(|w| w.data.len()).sum();
    println!("{mode}: {label}");
    println!(
        "Active profile: {} ({})",
        cfg.active_profile,
        profile_label(cfg.active_profile)
    );
    println!(
        "EEPROM writes: {} blocks ({total_bytes} bytes)",
        plan.writes.len()
    );
    println!(
        "Macro bytes used: {} / {} bytes",
        plan.macro_bytes_used, plan.macro_limit
    );
}

fn cmd_import(args: ImportArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ini::import_x806_bundle(
        &args.config_ini,
        args.macros_dir.as_deref(),
        args.pubg_dat.as_deref(),
        args.const_macro.as_deref(),
    )?;
    let toml_text = toml_codec::to_toml_string(&cfg)?;
    match args.out {
        Some(out_path) => {
            if let Some(parent) = out_path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out_path, toml_text)?;
            println!(
                "Imported '{}' -> '{}' ({} macro group(s), {} PUBG macro(s))",
                args.config_ini.display(),
                out_path.display(),
                cfg.macros.groups.len(),
                cfg.macros.pubg_macros.len()
            );
        }
        None => {
            print!("{toml_text}");
        }
    }
    Ok(())
}

fn cmd_export(args: ExportArgs) -> Result<(), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(&args.config_toml)?;
    let cfg = toml_codec::from_toml_str(&text)?;
    let macros_dir = args.macros_dir.unwrap_or_else(|| {
        args.out
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("macro")
    });
    ini::export_x806_bundle(&cfg, &args.out, &macros_dir)?;
    println!(
        "Exported '{}' -> '{}' (macros in '{}')",
        args.config_toml.display(),
        args.out.display(),
        macros_dir.display()
    );
    Ok(())
}

fn cmd_macro_inspect(file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(file)?;
    let macros = dat::parse_dat(&bytes)?;
    println!("File: {} ({} macro(s))", file.display(), macros.len());
    for (idx, mac) in macros.iter().enumerate() {
        println!(
            "\n[{idx}] \"{}\" | repeat={} | finish_cycle={} | opt=({}, {}) | events={} | led_blocks={}",
            mac.name,
            format_repeat_mode(mac.repeat_mode),
            mac.finish_cycle,
            mac.opt_flag,
            mac.opt_val,
            mac.events.len(),
            mac.led_blocks.len()
        );
        for (ev_idx, ev) in mac.events.iter().enumerate() {
            println!(
                "  {ev_idx:03}: {:<28} delay={} ms",
                format_event_kind(&ev.kind),
                ev.delay_ms
            );
        }
    }
    Ok(())
}

fn cmd_udev() {
    println!("# /etc/udev/rules.d/99-x806.rules");
    print!("{}", UDEV_RULES);
    if !UDEV_RULES.ends_with('\n') {
        println!();
    }
    println!();
    println!("# Install on Linux for non-root hidraw access:");
    println!("sudo cp udev/99-x806.rules /etc/udev/rules.d/99-x806.rules");
    println!("sudo udevadm control --reload-rules && sudo udevadm trigger");
}

fn default_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let proj = ProjectDirs::from("", "", "x806")
        .ok_or_else(|| X806Error::Toml("unable to resolve OS config directory".into()))?;
    Ok(proj.config_dir().join("config.toml"))
}

fn load_config_auto(
    path: &Path,
    macros_dir: Option<&Path>,
) -> Result<DeviceConfig, Box<dyn std::error::Error>> {
    let is_ini = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ini"));
    if is_ini {
        Ok(ini::import_x806_bundle(path, macros_dir, None, None)?)
    } else {
        let text = fs::read_to_string(path)?;
        Ok(toml_codec::from_toml_str(&text)?)
    }
}

fn profile_label(profile: u8) -> &'static str {
    match profile {
        0 => "Game I",
        2 => "Game II",
        3 => "Game III",
        1 => "Slot 1 (unused)",
        _ => "Unknown",
    }
}

fn format_repeat_mode(mode: RepeatMode) -> String {
    match mode {
        RepeatMode::Once => "Once".to_string(),
        RepeatMode::Count(n) => format!("Count({n})"),
        RepeatMode::UntilRelease => "UntilRelease".to_string(),
        RepeatMode::UntilClick => "UntilClick".to_string(),
    }
}

fn format_event_kind(kind: &MacroEventKind) -> String {
    match *kind {
        MacroEventKind::KeyDown { hid } => {
            format!("KeyDown {} (0x{hid:02X})", hid_display_name(hid))
        }
        MacroEventKind::KeyUp { hid } => {
            format!("KeyUp {} (0x{hid:02X})", hid_display_name(hid))
        }
        MacroEventKind::Delay => "Delay".to_string(),
        MacroEventKind::MoveRelative { dx, dy } => format!("MoveRelative dx={dx}, dy={dy}"),
        MacroEventKind::DpiSet { dpi_code } => format!("DpiSet code={dpi_code}"),
        MacroEventKind::DpiRestore { dpi_code } => format!("DpiRestore code={dpi_code}"),
        MacroEventKind::LedAnim { channel, index } => {
            format!("LedAnim ch={channel}, idx={index}")
        }
        MacroEventKind::Raw {
            event_type,
            hid,
            arg1,
            arg2,
        } => format!("Raw type={event_type} hid=0x{hid:02X} ({arg1}, {arg2})"),
    }
}
