#[macro_use]
extern crate serde;
#[macro_use]
extern crate wasm_bindgen;

use crate::project::*;
use crate::file::*;
use std::fmt::{Display, Formatter};

pub mod file;
pub mod project;

#[derive(Debug, Deserialize, Serialize)]
pub struct Empty;

transport! {
    ProjectList|project_list: Empty => ProjectList;
    ProjectInfo|project_info: ID => ProjectInfo;
    GetFile|get_file: GetFile => File;
    Flush|flush: Flush => FlushResult;
}

#[macro_export]
macro_rules! transport {
    ($($name:ident|$fun_name:ident : $req:ident => $resp:ident;)*) => {

        #[derive(Debug, Serialize)]
        pub enum SentRequest<'a> {
             $(
                $name(&'a $req),
             )*
        }

        #[derive(Debug, Deserialize)]
        pub enum ReceivedRequest {
             $(
                $name($req),
             )*
        }

        #[derive(Debug, Deserialize, Serialize)]
        pub enum Response {
             $(
                $name($resp),
             )*
        }

        $(
        impl<'a> From<&'a $req> for SentRequest<'a> {
            fn from(val: &'a $req) -> Self {
                Self::$name(val)
            }
        }
        )*

        $(
        impl std::convert::TryFrom<Response> for $resp {
            type Error = anyhow::Error;

            fn try_from(value: Response) -> Result<Self, Self::Error> {
                match value {
                    Response::$name(val) => Ok(val),
                    _ => anyhow::bail!("Type mismatch."),
                }
            }
        }
        )*

        pub trait OnRequest {
            $(
             fn $fun_name(&self, req: $req) -> Result<$resp, anyhow::Error>;
             )*

             fn handle(&self, buff: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
                let resp = bcs::from_bytes::<ReceivedRequest>(buff)
                    .map_err(|err| err.into())
                    .and_then(|req| {
                        match req {
                        $(
                            ReceivedRequest::$name(req) => self.$fun_name(req)
                            .map(|resp| Response::$name(resp)),
                        )*
                        }
                     })
                    .map_err(Error::from);

                Ok(bcs::to_bytes(&resp)?)
            }
        }

        pub async fn perform(url: &str, req: &SentRequest<'_>) -> Result<Response, anyhow::Error> {
            let resp = reqwest::Client::builder().build()?
                .post(url)
                .body(bcs::to_bytes(req)?)
                .send().await?;
            if resp.status().is_success() {
                bcs::from_bytes::<Result<Response, Error>>(resp.bytes().await?.as_ref())?
                .map_err(|err| anyhow::Error::msg(err.msg))
            } else {
                Err(anyhow::anyhow!("Failed to perform http request:{}", resp.status().as_str()))
            }
        }

        $(
            pub async fn $fun_name(url: &str, req: &$req) -> Result<$resp, anyhow::Error> {
                use std::convert::TryInto;
                let req = SentRequest::from(req);
                perform(url, &req).await?.try_into()
            }
        )*
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Error {
    pub msg: String,
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error {
            msg: err.to_string(),
        }
    }
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
