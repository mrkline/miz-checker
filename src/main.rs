mod logsetup;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;
use lazy_static::lazy_static;
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

    /// The DCS directory (TODO optional & load via registry)
    #[clap(short, long)]
    dcs: Utf8PathBuf
}

fn run() -> Result<()> {
    let args = Args::parse();
    logsetup::init_logger(args.verbose, args.color);

    let needed = parse_mission_liveries(&args.mission)?;
    let stock = find_stock_liveries(&args.dcs)?;

    let mut missing_liveries = false;

    for (vic, liveries) in needed {
        match stock.get(&vic) {
            Some(stock_liveries) => {
            },
            None => error!("Couldn't find ANY stock liveries for {vic}"),
        };
    }

    if missing_liveries {
        bail!("Missing liveries!");
    } else {
        Ok(())
    }
}

fn parse_mission_liveries(miz: &Utf8Path) -> Result<Liveries> {
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
        lua_livery_search(v, &mut liveries, k.clone())
            .with_context(|| format!("Couldn't parse {k} coalition"))?;
    }

    Ok(liveries)
}

fn map_miz(p: &Utf8Path) -> Result<Mmap> {
    let fd = std::fs::File::open(p).context("Couldn't open MIZ file")?;
    unsafe { Mmap::map(&fd) }.context("Couldn't map MIZ file")
}

fn lua_livery_search(t: mlua::Table, liveries: &mut Liveries, name: String) -> Result<()> {
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
                lua_livery_search(t, liveries, name)?;
            }
        }
    }
    Ok(())
}

fn find_stock_liveries(dcs: &Utf8Path) -> Result<Liveries> {
    let mut liveries = Liveries::new();
    dir_livery_search(dcs, &mut liveries)?;
    Ok(liveries)
}

fn dir_livery_search(dir: &Utf8Path, liveries: &mut Liveries) -> Result<()> {
    if !dir.is_dir() {
        return Ok(())
    }

    lazy_static! {
        static ref SOME_LIVERIES: Option<String> = Some("liveries".to_string());
    }

    if dir.file_name().map(|f| f.to_lowercase()) == *SOME_LIVERIES {
        trace!("Searching {dir} for stock liveries");
        for v in dir.read_dir_utf8()? {
            let v = v?;
            let v = v.path();
            if !v.is_dir() {
                continue;
            }
            let vehicle = v.file_name().unwrap().to_lowercase();
            trace!("\tFor {vehicle} found:");
            for l in v.read_dir_utf8()? {
                let l = l?;
                let l = l.path();
                if l.is_dir() {
                    let livery = l.file_name().unwrap().to_lowercase();
                    trace!("\t\t{livery}");
                    liveries.entry(vehicle.clone()).or_default().insert(livery);
                }
            }
        }
    } else {
        for e in dir.read_dir_utf8()? {
            let e = e?;
            dir_livery_search(e.path(), liveries)?;
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
