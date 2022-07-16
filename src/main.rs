mod logsetup;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;
use log::*;
use memmap::Mmap;
use mlua::Lua;

use std::collections::{BTreeMap, BTreeSet};

// A set of livery IDs required for each vehicle type
type Liveries = BTreeMap<String, BTreeSet<String>>;

#[derive(Parser, Debug)]
struct Args {
    /// Verbosity (-v, -vv, -vvv, etc.)
    #[clap(short, long, parse(from_occurrences))]
    verbose: u8,

    #[clap(short, long, arg_enum, default_value = "auto")]
    color: logsetup::Color,

    /// The directory containing the previous BMS config
    mission: Utf8PathBuf,
}

fn run() -> Result<()> {
    let args = Args::parse();
    logsetup::init_logger(args.verbose, args.color);

    parse_liveries(&args.mission)?;
    Ok(())
}

fn parse_liveries(miz: &Utf8Path) -> Result<()> {
    let miz = map_miz(miz)?;

    let lua = Lua::new();
    lua.load(&*miz).exec().context("Couldn't parse mission")?;

    let loaded_miz: mlua::Table = lua
        .globals()
        .get("mission")
        .and_then(|m: mlua::Table| m.get("coalition"))
        .context("Couldn't parse mission.coalitions")?;

    let mut liveries = Liveries::new();

    for coalition in loaded_miz.pairs::<mlua::String, mlua::Table>() {
        let (k, v) = coalition?;
        let k = k.to_string_lossy().to_string();
        livery_search(v, &mut liveries, k.clone())
            .with_context(|| format!("Couldn't parse {k} coalition"))?;
    }

    println!("Required liveries:");
    println!("{liveries:#?}");

    Ok(())
}

fn map_miz(p: &Utf8Path) -> Result<Mmap> {
    let fd = std::fs::File::open(p).context("Couldn't open MIZ file")?;
    unsafe { Mmap::map(&fd) }.context("Couldn't map MIZ file")
}

fn livery_search(t: mlua::Table, liveries: &mut Liveries, name: String) -> Result<()> {
    trace!("Searching {name}");
    let livery_id: mlua::Value = t.get("livery_id")?;
    let unit_type: mlua::Value = t.get("type")?;
    if let (mlua::Value::String(id), mlua::Value::String(ut)) = (livery_id, unit_type) {
        let id = id.to_string_lossy().to_lowercase();
        let ut = ut.to_string_lossy().to_lowercase();
        liveries.entry(ut).or_default().insert(id);
    } else {
        for pair in t.pairs::<mlua::Value, mlua::Value>() {
            let (k, v) = pair?;
            if let mlua::Value::Table(t) = v {
                let mut name = name.clone();
                name.push('.');
                match k {
                    mlua::Value::String(s) => name += &s.to_string_lossy(),
                    mlua::Value::Integer(i) => name += &i.to_string(),
                    _ => unreachable!(),
                };
                livery_search(t, liveries, name)?;
            }
        }
    }
    Ok(())
}

fn main() {
    run().unwrap_or_else(|e| {
        error!("{:?}", e);
        std::process::exit(1);
    });
}
