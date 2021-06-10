mod client;

use crate::{Net, Block, BytesForBlock};
use move_core_types::language_storage::{ModuleId, StructTag};
use http::Uri;
use anyhow::Result;
use lang::compiler::dialects::Dialect;
use crate::dnode::client::data_request;
use move_core_types::account_address::AccountAddress;

pub struct DnodeNet {
    pub(crate) dialect: Box<dyn Dialect>,
    pub(crate) uri: Uri,
}

impl Net for DnodeNet {
    fn get_module(
        &self,
        module_id: &ModuleId,
        height: &Option<Block>,
    ) -> Result<Option<BytesForBlock>> {
        let address = self.dialect.adapt_address_to_target(*module_id.address());
        let bytes = data_request(&address, &module_id.access_vector(), &self.uri, height).ok();
        match bytes {
            None => Ok(None),
            Some(mut bytes) => {
                self.dialect.adapt_to_basis(&mut bytes.0)?;
                Ok(Some(BytesForBlock(bytes.0, bytes.1)))
            }
        }
    }

    fn get_resource(
        &self,
        address: &AccountAddress,
        tag: &StructTag,
        height: &Option<Block>,
    ) -> Result<Option<BytesForBlock>> {
        let address = self.dialect.adapt_address_to_target(*address);
        let access_vector = tag.access_vector();
        let bytes = data_request(&address, &access_vector, &self.uri, height).ok();
        match bytes {
            None => Ok(None),
            Some(mut bytes) => {
                self.dialect.adapt_to_basis(&mut bytes.0)?;
                Ok(Some(BytesForBlock(bytes.0, bytes.1)))
            }
        }
    }

    fn dialect(&self) -> &dyn Dialect {
        self.dialect.as_ref()
    }
}