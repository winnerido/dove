use core::mem;
use std::collections::HashMap;
use std::fs;
use std::ffi::OsStr;
use std::fs::remove_file;
use std::path::{PathBuf, Path};
use anyhow::Error;
use structopt::StructOpt;
use anyhow::Result;
use move_binary_format::access::ModuleAccess;
use move_binary_format::CompiledModule;

use move_cli::Command as MoveCommand;
use move_cli::package::cli::PackageCommand;
use move_cli::run_cli;
use move_core_types::language_storage::ModuleId;

use crate::context::Context;
use serde::{Serialize, Deserialize};

#[derive(StructOpt, Debug, Default)]
#[structopt(setting(structopt::clap::AppSettings::ColoredHelp))]
pub struct Deploy {
    // Names of modules to exclude from the package process.
    // Modules are taken from the <PROJECT_PATH>/build/<PROJECT_NAME>/bytecode_modules directory.
    // The names are case-insensitive and can be specified with an extension.mv or without it.
    // --modules_exclude NAME_1 NAME_2 NAME_3
    #[structopt(
        help = "Names of modules to exclude from the package process.",
        long = "modules_exclude"
    )]
    modules_exclude: Vec<String>,

    #[structopt(
        help = "File name of the resulting .pac file.",
        short = "o",
        long = "output"
    )]
    output: Option<String>,
}

impl Deploy {
    pub fn apply(&mut self, ctx: &mut Context) -> Result<()> {
        // Run `dove package build` first to build all necessary artifacts.
        run_dove_package_build(ctx)?;

        // packaging of modules
        self.bundle_modules_into_pac(ctx)?;

        Ok(())
    }
}

impl Deploy {
    fn bundle_modules_into_pac(&self, ctx: &Context) -> Result<()> {
        // Path to the output file
        let output_file_path = ctx
            .bundles_output_path(
                self.output
                    .as_deref()
                    .unwrap_or_else(|| ctx.manifest.package.name.as_str()),
            )?
            .with_extension("pac");
        if output_file_path.exists() {
            remove_file(&output_file_path)?;
        }

        // Search for modules
        let bytecode_modules_path =
            get_bytecode_modules_path(&ctx.project_root_dir, &ctx.manifest.package.name)
                .unwrap_or_default();

        let mut pac = ModulePackage::default();

        for module in bytecode_modules_path {
            let module_name = module.file_name().map(|name| {
                let name = name.to_string_lossy();
                name[0..name.len() - ".mv".len()].to_string()
            }).ok_or_else(|| anyhow!("Failed to package move module: '{:?}'. File with .mv extension was expected.", module))?;
            if self.modules_exclude.contains(&module_name) {
                continue;
            }
            pac.put(fs::read(&module)?);
        }

        pac.sort()?;

        fs::write(&output_file_path, pac.encode()?)?;

        println!(
            "Modules are packed {}",
            output_file_path
                .canonicalize()
                .unwrap_or_default()
                .display()
        );
        Ok(())
    }
}

/// Return file paths from ./PROJECT_FOLDER/build/PROJECT_NAME/bytecode_modules
/// Only with the .mv extension
fn get_bytecode_modules_path(project_dir: &Path, project_name: &str) -> Result<Vec<PathBuf>> {
    let path = project_dir
        .join("build")
        .join(project_name)
        .join("bytecode_modules");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let list = fs::read_dir(path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map(|list| {
            list.into_iter()
                .filter(|path| path.is_file() && path.extension() == Some(OsStr::new("mv")))
                .collect::<Vec<_>>()
        })?;
    Ok(list)
}

pub fn run_dove_package_build(ctx: &mut Context) -> Result<()> {
    let build_cmd = MoveCommand::Package {
        cmd: PackageCommand::Build {},
    };
    run_cli(
        ctx.native_functions.clone(),
        &ctx.cost_table,
        &ctx.error_descriptions,
        &ctx.move_args,
        &build_cmd,
    )
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct ModulePackage {
    modules: Vec<Vec<u8>>,
}

impl ModulePackage {
    pub fn put(&mut self, module: Vec<u8>) {
        self.modules.push(module);
    }

    pub fn sort(&mut self) -> Result<(), Error> {
        let mut modules = Vec::with_capacity(self.modules.len());
        mem::swap(&mut self.modules, &mut modules);

        let mut modules = modules
            .into_iter()
            .map(|bytecode| {
                CompiledModule::deserialize(&bytecode)
                    .map(|unit| (unit.self_id(), (bytecode, unit)))
                    .map_err(|_| anyhow!("Failed to deserialize move module."))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        let mut ids_list: Vec<_> = modules.keys().cloned().collect();
        ids_list.sort();

        for id in ids_list {
            self.write_sub_tree(&id, &mut modules);
        }

        Ok(())
    }

    fn write_sub_tree(
        &mut self,
        id: &ModuleId,
        modules: &mut HashMap<ModuleId, (Vec<u8>, CompiledModule)>,
    ) {
        if let Some((bytecode, unit)) = modules.remove(id) {
            let deps = Self::take_deps(id, &unit);
            for dep in deps {
                self.write_sub_tree(&dep, modules);
            }
            println!("Packing '{}'...", id.name());
            self.modules.push(bytecode);
        }
    }

    fn take_deps(id: &ModuleId, unit: &CompiledModule) -> Vec<ModuleId> {
        unit.module_handles()
            .iter()
            .map(|hdl| unit.module_id_for_handle(hdl))
            .filter(|dep_id| dep_id != id)
            .collect()
    }

    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        bcs::to_bytes(&self).map_err(|err| err.into())
    }
}