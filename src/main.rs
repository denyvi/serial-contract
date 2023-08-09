#![warn(rust_2018_idioms)]
#[subxt::subxt(runtime_metadata_path = "./resources/goro.metadata")]
pub mod goro {}

use futures::stream::StreamExt;
use std::{env, io, str};
use tokio_util::codec::{Decoder, Encoder};

use bytes::BytesMut;
use tokio_serial::SerialPortBuilderExt;

use crate::goro::runtime_types::sp_weights::weight_v2::Weight;
use contract_transcode::ContractMessageTranscoder;
use goro::tx as extrinsic;
use ss58_registry::Ss58AddressFormatRegistry;
use subxt::ext::sp_core::crypto::set_default_ss58_version;
use subxt::ext::sp_core::sr25519::Pair as Sr25519KeyPair;
use subxt::ext::sp_core::Pair;
use subxt::ext::sp_runtime::app_crypto::Ss58Codec;
use subxt::ext::sp_runtime::AccountId32;
use subxt::tx::PairSigner;
use subxt::utils::MultiAddress;
use subxt::{OnlineClient, PolkadotConfig};

pub const CALLER_SIGNER: &str = "5ExcvnRUfE9dWBgma5DCVeENgiq2jEo1cY4pW7J8yqvjTE3C";
pub const CALLER_PHRASE: &str =
    "flag repeat rubber donate track dish author target company ritual frame report";
pub const CONTRACT_ADDRESS: &str = "gr2ChZDqPRYEDF2zU9tPDyUmBeFv2mGsu2Xu4Uocf2e8C6oNq";
pub const CONTRACT_METADATA: &str = "./resources/rantai_suplai.json";
// pub const CONTRACT_METHOD: &str = "register_product";
pub const CONTRACT_METHOD: &str = "register_out_factory";
// pub const CONTRACT_METHOD: &str = "register_supplier_arrived";
// pub const CONTRACT_METHOD: &str = "register_sold_product";
pub const LIMIT_PROOF_SIZE: u64 = 131_072;
pub const LIMIT_REF_TIME: u64 = 8_589_934_592;

#[cfg(unix)]
const DEFAULT_TTY: &str = "/dev/ttyUSB0";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM1";

struct LineCodec;

impl Decoder for LineCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let newline = src.as_ref().iter().position(|b| *b == b'\n');
        if let Some(n) = newline {
            let line = src.split_to(n + 1);
            return match str::from_utf8(line.as_ref()) {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Err(io::Error::new(io::ErrorKind::Other, "Invalid String")),
            };
        }
        Ok(None)
    }
}

impl Encoder<String> for LineCodec {
    type Error = io::Error;

    fn encode(&mut self, _item: String, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    set_default_ss58_version(Ss58AddressFormatRegistry::GoroAccount.into());
    let sender_as_keypair = Sr25519KeyPair::from_phrase(CALLER_PHRASE, None)?;
    let contract_as_account = AccountId32::from_ss58check(CONTRACT_ADDRESS)?;

    let mut args = env::args();
    let tty_path = args.nth(1).unwrap_or_else(|| DEFAULT_TTY.into());

    let mut port = tokio_serial::new(tty_path, 115200).open_native_async()?;

    #[cfg(unix)]
    port.set_exclusive(false)
        .expect("Unable to set serial port exclusive to false");

    let mut reader = LineCodec.framed(port);

    while let Some(line_result) = reader.next().await {
        let line = line_result.expect("Failed to read line");
        let parsed_line: Vec<&str> = line.split("-").collect();
        let batch_id = parsed_line[0].trim();
        let product_id = parsed_line[1].trim();
        let quote_char = "\"";
        let mod_batch_id = format!("{}{}{}", quote_char,batch_id,quote_char);
        let mod_product_id = format!("{}{}{}", quote_char,product_id,quote_char);

        let msg_args = [mod_batch_id, mod_product_id];
        
        println!("{:?}", msg_args);

        // Message
        let sender_as_signer =
            PairSigner::<PolkadotConfig, Sr25519KeyPair>::new(sender_as_keypair.clone().0);
        let gas_limit = Weight {
            proof_size: LIMIT_PROOF_SIZE,
            ref_time: LIMIT_REF_TIME,
        };
        let contract_transcoder = ContractMessageTranscoder::load(CONTRACT_METADATA)?;
        println!("sampai sini 1");
        let write_message = contract_transcoder.encode(CONTRACT_METHOD, &msg_args)?;

        println!("sampai sini 2");
        let contract_as_multiaddress = MultiAddress::from(contract_as_account.clone());
        println!("sampai sini 3");
        let extrinsic_message = extrinsic().contracts().call(
            contract_as_multiaddress,
            0,
            gas_limit,
            None,
            write_message,
        );
        println!("sampai sini 4");

        // API
        let extrinsic_client =
            OnlineClient::<PolkadotConfig>::from_url("wss://main-00.goro.network:443").await?;
        let extrinsic_progress = extrinsic_client
            .tx()
            .sign_and_submit_then_watch_default(&extrinsic_message, &sender_as_signer)
            .await?;
        println!("\n[Contract Call Sent]");
        println!("=> hash     : {}", extrinsic_progress.extrinsic_hash());

        // Event filtering
        let extrinsic_events = extrinsic_progress
            .wait_for_in_block()
            .await?
            .wait_for_success()
            .await?;

        if let Some(contract_event) =
            extrinsic_events.find_first::<goro::contracts::events::Called>()?
        {
            let goro::contracts::events::Called { caller, contract } = contract_event;

            println!("\n[Contract Called]");
            println!("=> caller   : {}", AccountId32::new(caller.0));
            println!("=> contract : {}", AccountId32::new(contract.0));
        }
    }
    Ok(())
}
