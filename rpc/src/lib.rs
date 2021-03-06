// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Tetcoin.

// Tetcoin is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Tetcoin is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Tetcoin.  If not, see <http://www.gnu.org/licenses/>.

//! Tetcoin-specific RPCs implementation.

#![warn(missing_docs)]

use std::sync::Arc;

use tetcoin_primitives::v0::{Block, BlockNumber, AccountId, Nonce, Balance, Hash};
use tp_api::ProvideRuntimeApi;
use txpool_api::TransactionPool;
use tp_block_builder::BlockBuilder;
use tp_blockchain::{HeaderBackend, HeaderMetadata, Error as BlockChainError};
use tp_consensus::SelectChain;
use tp_consensus_babe::BabeApi;
use tp_keystore::SyncCryptoStorePtr;
use tc_client_api::AuxStore;
use tc_client_api::light::{Fetcher, RemoteBlockchain};
use tc_consensus_babe::Epoch;
use tc_finality_grandpa::FinalityProofProvider;
use tc_sync_state_rpc::{SyncStateRpcApi, SyncStateRpcHandler};
pub use tc_rpc::{DenyUnsafe, SubscriptionTaskExecutor};

/// A type representing all RPC extensions.
pub type RpcExtension = tetsy_jsonrpc_core::IoHandler<tc_rpc::Metadata>;

/// Light client extra dependencies.
pub struct LightDeps<C, F, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Remote access to the blockchain (async).
	pub remote_blockchain: Arc<dyn RemoteBlockchain<Block>>,
	/// Fetcher instance.
	pub fetcher: Arc<F>,
}

/// Extra dependencies for BABE.
pub struct BabeDeps {
	/// BABE protocol config.
	pub babe_config: tc_consensus_babe::Config,
	/// BABE pending epoch changes.
	pub shared_epoch_changes: tc_consensus_epochs::SharedEpochChanges<Block, Epoch>,
	/// The keystore that manages the keys of the node.
	pub keystore: SyncCryptoStorePtr,
}

/// Dependencies for GRANDPA
pub struct GrandpaDeps<B> {
	/// Voting round info.
	pub shared_voter_state: tc_finality_grandpa::SharedVoterState,
	/// Authority set info.
	pub shared_authority_set: tc_finality_grandpa::SharedAuthoritySet<Hash, BlockNumber>,
	/// Receives notifications about justification events from Grandpa.
	pub justification_stream: tc_finality_grandpa::GrandpaJustificationStream<Block>,
	/// Executor to drive the subscription manager in the Grandpa RPC handler.
	pub subscription_executor: tc_rpc::SubscriptionTaskExecutor,
	/// Finality proof provider.
	pub finality_provider: Arc<FinalityProofProvider<B, Block>>,
}

/// Full client dependencies
pub struct FullDeps<C, P, SC, B> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// The SelectChain Strategy
	pub select_chain: SC,
	/// A copy of the chain spec.
	pub chain_spec: Box<dyn tc_chain_spec::ChainSpec>,
	/// Whether to deny unsafe calls
	pub deny_unsafe: DenyUnsafe,
	/// BABE specific dependencies.
	pub babe: BabeDeps,
	/// GRANDPA specific dependencies.
	pub grandpa: GrandpaDeps<B>,
}

/// Instantiate all RPC extensions.
pub fn create_full<C, P, SC, B>(deps: FullDeps<C, P, SC, B>) -> RpcExtension where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + AuxStore +
		HeaderMetadata<Block, Error=BlockChainError> + Send + Sync + 'static,
	C::Api: fabric_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: noble_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: BabeApi<Block>,
	C::Api: BlockBuilder<Block>,
	P: TransactionPool + Sync + Send + 'static,
	SC: SelectChain<Block> + 'static,
	B: tc_client_api::Backend<Block> + Send + Sync + 'static,
	B::State: tc_client_api::StateBackend<tp_runtime::traits::HashFor<Block>>,
{
	use fabric_rpc_system::{FullSystem, SystemApi};
	use noble_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApi};
	use tc_finality_grandpa_rpc::{GrandpaApi, GrandpaRpcHandler};
	use tc_consensus_babe_rpc::BabeRpcHandler;

	let mut io = tetsy_jsonrpc_core::IoHandler::default();
	let FullDeps {
		client,
		pool,
		select_chain,
		chain_spec,
		deny_unsafe,
		babe,
		grandpa,
	} = deps;
	let BabeDeps {
		keystore,
		babe_config,
		shared_epoch_changes,
	} = babe;
	let GrandpaDeps {
		shared_voter_state,
		shared_authority_set,
		justification_stream,
		subscription_executor,
		finality_provider,
	} = grandpa;

	io.extend_with(
		SystemApi::to_delegate(FullSystem::new(client.clone(), pool, deny_unsafe))
	);
	io.extend_with(
		TransactionPaymentApi::to_delegate(TransactionPayment::new(client.clone()))
	);
	io.extend_with(
		tc_consensus_babe_rpc::BabeApi::to_delegate(
			BabeRpcHandler::new(
				client.clone(),
				shared_epoch_changes.clone(),
				keystore,
				babe_config,
				select_chain,
				deny_unsafe,
			)
		)
	);
	io.extend_with(
		GrandpaApi::to_delegate(GrandpaRpcHandler::new(
			shared_authority_set.clone(),
			shared_voter_state,
			justification_stream,
			subscription_executor,
			finality_provider,
		))
	);
	io.extend_with(
		SyncStateRpcApi::to_delegate(SyncStateRpcHandler::new(
			chain_spec,
			client,
			shared_authority_set,
			shared_epoch_changes,
			deny_unsafe,
		))
	);
	io
}

/// Instantiate all RPC extensions for light node.
pub fn create_light<C, P, F>(deps: LightDeps<C, F, P>) -> RpcExtension
	where
		C: ProvideRuntimeApi<Block>,
		C: HeaderBackend<Block>,
		C: Send + Sync + 'static,
		C::Api: fabric_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
		C::Api: noble_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
		P: TransactionPool + Sync + Send + 'static,
		F: Fetcher<Block> + 'static,
{
	use fabric_rpc_system::{LightSystem, SystemApi};

	let LightDeps {
		client,
		pool,
		remote_blockchain,
		fetcher,
	} = deps;
	let mut io = tetsy_jsonrpc_core::IoHandler::default();
	io.extend_with(
		SystemApi::<Hash, AccountId, Nonce>::to_delegate(LightSystem::new(client, remote_blockchain, fetcher, pool))
	);
	io
}
