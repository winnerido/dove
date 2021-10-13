use anyhow::{Error, bail};
use itertools::{Itertools, Either};
use move_core_types::language_storage::TypeTag;
use move_core_types::identifier::Identifier;
use move_core_types::account_address::AccountAddress;
use move_lang::compiled_unit::CompiledUnit;
use lang::tx::fn_call::{select_function, prepare_function_signature};
use lang::tx::model::{Signers, Transaction, Call, EnrichedTransaction};
use crate::compiler::build_base;
use crate::compiler::interact::CompilerInteract;
use crate::storage::web::WebStorage;
use crate::loader::Loader;
use crate::deps::resolver::DependencyResolver;
use crate::tx::ProjectData;
use crate::tx::resolver::{find_script, find_module_function};

pub(crate) fn make_script_call(
    // Project data
    project_data: &ProjectData,
    // script name
    name: Identifier,
    // Generics for script
    type_tag: Vec<TypeTag>,
    // arguments for the function
    args: Vec<String>,
    // At what index is the script located
    index_in_source_map: Option<String>,
) -> Result<EnrichedTransaction, Error> {
    let store = WebStorage::new_in_family("dove_cache_")?;
    let loader = Loader::new(project_data.chain_api.to_string());
    let account_address = project_data.account_address.clone();
    let scripts = find_script(project_data, &name, index_in_source_map)?;

    let (finded_index, meta) = select_function(
        scripts.clone(),
        account_address,
        &type_tag,
        &args,
        &project_data.cfg,
    )?;
    let (signers, args) = prepare_function_signature(
        &meta.parameters,
        &args,
        !project_data.cfg.deny_signers_definition,
        account_address.clone(),
    )?;
    // Creating transaction
    let (signers, mut tx) = match signers {
        Signers::Explicit(signers) => (
            signers,
            Transaction::new_script_tx(vec![], vec![], args, type_tag)?,
        ),
        Signers::Implicit(signers) => (
            vec![],
            Transaction::new_script_tx(signers, vec![], args, type_tag)?,
        ),
    };

    // @todo Used to run
    // let (_, interface) = ctx.build_index()?;

    // Building project
    let sender = account_address.to_string();
    let resolver = DependencyResolver::new(project_data.dialect.as_ref(), loader, store);
    let mut interact = CompilerInteract::new(
        project_data.dialect.as_ref(),
        &sender,
        project_data.source_map.clone(),
        resolver,
    );
    let (modules, script): (Vec<_>, Vec<_>) =
        build_base(&mut interact, project_data.source_map.clone())?
            .into_iter()
            .filter_map(|unit| match unit {
                CompiledUnit::Module { module, .. } => Some(Either::Left(module)),
                CompiledUnit::Script {
                    loc, key, script, ..
                } => {
                    if loc.file == finded_index && key == name.as_str() {
                        Some(Either::Right(script))
                    } else {
                        None
                    }
                }
            })
            .partition_map(|u| u);

    if script.is_empty() {
        bail!("The script {:?} could not be compiled", finded_index);
    }

    let mut buff = Vec::new();
    script[0].serialize(&mut buff)?;
    match &mut tx.inner_mut().call {
        Call::Script { code, .. } => *code = buff,
        Call::ScriptFunction { .. } => {
            // no-op
        }
    }

    Ok(if project_data.cfg.exe_context {
        // @todo Used to run
        // modules.extend(interface.load_mv()?);
        EnrichedTransaction::Local {
            tx,
            signers,
            deps: modules,
        }
    } else {
        EnrichedTransaction::Global {
            tx,
            name: name.into_string(),
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn make_function_call(
    // Project data
    project_data: &ProjectData,
    // Module address
    module_address: AccountAddress,
    // module name
    module_name: Identifier,
    // function name
    function_name: Identifier,
    // Generics for function
    type_tag: Vec<TypeTag>,
    // arguments for function
    args: Vec<String>,
    // At what index is the script located
    source_index: Option<String>,
) -> Result<EnrichedTransaction, Error> {
    let functions = find_module_function(
        project_data,
        &module_address,
        &module_name,
        &function_name,
        source_index.as_ref(),
    )?;
    let account_address = project_data.account_address.clone();
    let (_, meta) = select_function(
        functions,
        account_address,
        &type_tag,
        &args,
        &project_data.cfg,
    )?;

    let (signers, args) = prepare_function_signature(
        &meta.parameters,
        &args,
        !project_data.cfg.deny_signers_definition,
        account_address,
    )?;
    let tx_name = format!("{}_{}", module_name, function_name);
    let (_signers, tx) = match signers {
        Signers::Explicit(signers) => (
            signers,
            Transaction::new_func_tx(
                vec![],
                module_address,
                module_name,
                function_name,
                args,
                type_tag,
            )?,
        ),
        Signers::Implicit(signers) => (
            vec![],
            Transaction::new_func_tx(
                signers,
                module_address,
                module_name,
                function_name,
                args,
                type_tag,
            )?,
        ),
    };

    Ok(if project_data.cfg.exe_context {
        // @todo for run
        anyhow::bail!("@todo make_function_call exe_context")
        // let modules_dir = ctx.str_path_for(&ctx.manifest.layout.modules_dir)?;
        //
        // let (_, interface) = ctx.build_index()?;
        // let mut deps = move_build(
        //     ctx,
        //     &[modules_dir],
        //     &[interface.dir.to_string_lossy().into_owned()],
        // )?
        // .into_iter()
        // .filter_map(|m| match m {
        //     CompiledUnit::Module { module, .. } => Some(module),
        //     CompiledUnit::Script { .. } => None,
        // })
        // .collect::<Vec<_>>();
        // deps.extend(interface.load_mv()?);
        // EnrichedTransaction::Local { tx, signers, deps }
    } else {
        EnrichedTransaction::Global { tx, name: tx_name }
    })
}
