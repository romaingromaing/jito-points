use anchor_lang::prelude::Pubkey;
use anyhow::Error;
use mango_v4::state::{Bank, MangoAccountValue};
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_config::RpcProgramAccountsConfig;
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_client::{
    nonblocking::rpc_client::RpcClient as RpcClientAsync, rpc_config::RpcAccountInfoConfig,
};
use solana_sdk::pubkey;

const JITOSOL_TOKEN_INDEX: u16 = 501;

pub async fn fetch_mango_accounts_by_owner(
    rpc: &RpcClientAsync,
    program: Pubkey,
    group: Pubkey,
    owner: Pubkey,
) -> anyhow::Result<Vec<(Pubkey, MangoAccountValue)>> {
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                [243, 228, 247, 3, 169, 52, 175, 31].to_vec(), // mango discriminator
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(8, group.to_bytes().to_vec())),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(40, owner.to_bytes().to_vec())),
        ]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..RpcAccountInfoConfig::default()
        },
        ..RpcProgramAccountsConfig::default()
    };
    rpc.get_program_accounts_with_config(&program, config)
        .await?
        .into_iter()
        .map(|(key, account)| Ok((key, MangoAccountValue::from_bytes(&account.data[8..])?)))
        .collect::<Result<Vec<_>, _>>()
}

async fn fetch_banks(
    rpc: &RpcClientAsync,
    program: Pubkey,
    group: Pubkey,
) -> anyhow::Result<Vec<(Pubkey, Bank)>> {
    let filters = vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
        8,
        group.to_bytes().to_vec(),
    ))];
    let account_type_filter = RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
        0,
        [142, 49, 166, 242, 50, 66, 97, 188].to_vec(), // bank discriminator
    ));
    let config = RpcProgramAccountsConfig {
        filters: Some([vec![account_type_filter], filters].concat()),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..RpcAccountInfoConfig::default()
        },
        ..RpcProgramAccountsConfig::default()
    };
    rpc.get_program_accounts_with_config(&program, config)
        .await?
        .into_iter()
        .map(|(key, account)| Ok((key, *bytemuck::from_bytes::<Bank>(&(&account.data[8..])))))
        .collect()
}

pub async fn fetch_jitosol_bank(
    rpc: &RpcClientAsync,
    program: Pubkey,
    group: Pubkey,
) -> anyhow::Result<Bank> {
    let token_banks = fetch_banks(&rpc, program, group).await.unwrap();
    match token_banks
        .iter()
        .find(|(_, b)| b.token_index == JITOSOL_TOKEN_INDEX)
    {
        Some(jb) => Ok(jb.1),
        None => Err(Error::msg("JitoSol token bank not found")),
    }
}

pub async fn fetch_jitosol_exposure(
    rpc: &RpcClientAsync,
    program: Pubkey,
    group: Pubkey,
    owner_pk: Pubkey,
    jito_bank: Bank,
) -> anyhow::Result<f64> {
    let mut jitosol_amount = 0f64;

    let mango_account_tuples = fetch_mango_accounts_by_owner(&rpc, program, group, owner_pk)
        .await
        .unwrap();
    for (_, acct) in mango_account_tuples.iter() {
        match acct.token_position(JITOSOL_TOKEN_INDEX) {
            Ok(token_position) => {
                // token_position.ui is positive in the case of deposits and negative in the case of borrows
                jitosol_amount += token_position.ui(&jito_bank).to_num::<f64>().abs()
            }
            Err(_) => continue,
        }
    }
    Ok(jitosol_amount)
}

#[tokio::main]
async fn main() {
    let program = mango_v4::ID;
    let group = pubkey!("78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX");
    let owner_pk = pubkey!("Wallet Private Key Here");
    let rpc =
        RpcClientAsync::new("RPC HERE".to_string());

    let jito_bank = fetch_jitosol_bank(&rpc, program, group).await.unwrap();
    let jitosol_exposure = fetch_jitosol_exposure(&rpc, program, group, owner_pk, jito_bank)
        .await
        .unwrap();
    println!(
        "Mango accounts owned by {:?} have {:?} of JitoSol exposure",
        owner_pk, jitosol_exposure
    );
}
