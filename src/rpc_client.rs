use base64;
use hyper;
use serde_json;

use bitcoin::blockdata::block::BlockHeader;

use futures::{future, Future, Stream};

use utils::hex_to_u256_rev;

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Deserialize)]
pub struct GetHeaderResponse {
	pub hash: String,
	pub confirmations: u64,
	pub height: u32,
	pub version: u32,
	pub merkleroot: String,
	pub time: u32,
	pub nonce: u32,
	pub bits: String,
	pub previousblockhash: String,
}

impl GetHeaderResponse {
	pub fn to_block_header(&self) -> BlockHeader {
		BlockHeader {
			version: self.version,
			prev_blockhash: hex_to_u256_rev(&self.previousblockhash).unwrap(),
			merkle_root: hex_to_u256_rev(&self.merkleroot).unwrap(),
			time: self.time,
			bits: self.bits.parse().unwrap(),
			nonce: self.nonce,
		}
	}
}

pub struct RPCClient {
	basic_auth: String,
	uri: String,
	id: AtomicUsize,
	client: hyper::Client<hyper::client::HttpConnector, hyper::Body>,
}

impl RPCClient {
	pub fn new(user_auth: &str, host_port: &str) -> Self {
		Self {
			basic_auth: "Basic ".to_string() + &base64::encode(user_auth),
			uri: "http://".to_string() + host_port,
			id: AtomicUsize::new(0),
			client: hyper::Client::new(),
		}
	}

	/// params entries must be pre-quoted if appropriate
	pub fn make_rpc_call(&self, method: &str, params: &[&str]) -> impl Future<Item=serde_json::Value, Error=()> {
		let mut request = hyper::Request::post(&self.uri);
		let auth: &str = &self.basic_auth;
		request.header("Authorization", auth);
		let mut param_str = String::new();
		for (idx, param) in params.iter().enumerate() {
			param_str += param;
			if idx != params.len() - 1 {
				param_str += ",";
			}
		}
		self.client.request(request.body(hyper::Body::from("{\"method\":\"".to_string() + method + "\",\"params\":[" + &param_str + "],\"id\":" + &self.id.fetch_add(1, Ordering::AcqRel).to_string() + "}")).unwrap()).map_err(|_| {
			println!("Failed to connect to RPC server!");
			()
		}).and_then(|res| {
			if res.status() != hyper::StatusCode::OK {
				println!("Failed to get RPC server response (probably bad auth)!");
				future::Either::A(future::err(()))
			} else {
				future::Either::B(res.into_body().concat2().map_err(|_| {
					println!("Failed to load RPC server response!");
					()
				}).and_then(|body| {
					let v: serde_json::Value = match serde_json::from_slice(&body) {
						Ok(v) => v,
						Err(_) => {
							println!("Failed to parse RPC server response!");
							return future::err(())
						},
					};
					if !v.is_object() {
						println!("Failed to parse RPC server response!");
						return future::err(());
					}
					let v_obj = v.as_object().unwrap();
					if v_obj.get("error") != Some(&serde_json::Value::Null) {
						println!("Failed to parse RPC server response!");
						return future::err(());
					}
					if let Some(res) = v_obj.get("result") {
						future::result(Ok((*res).clone()))
					} else {
						println!("Failed to parse RPC server response!");
						return future::err(());
					}
				}))
			}
		})
	}

	pub fn get_header(&self, header_hash: &str) -> impl Future<Item=GetHeaderResponse, Error=()> {
		let param = "\"".to_string() + header_hash + "\"";
		self.make_rpc_call("getblockheader", &[&param]).and_then(|v| {
			let deser_res: Result<GetHeaderResponse, _> = serde_json::from_value(v);
			match deser_res {
				Ok(resp) => Ok(resp),
				Err(_) => {
					println!("Got invalid header message from RPC server!");
					Err(())
				},
			}
		})
	}
}