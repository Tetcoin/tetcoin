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

//! Auxillary struct/enums for tetcoin runtime.

use fabric_support::traits::{OnUnbalanced, Imbalance, Currency};
use crate::NegativeImbalance;

/// Logic for the author to get a portion of fees.
pub struct ToAuthor<R>(tetcore_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for ToAuthor<R>
where
	R: noble_balances::Config + noble_authorship::Config,
	<R as fabric_system::Config>::AccountId: From<primitives::v1::AccountId>,
	<R as fabric_system::Config>::AccountId: Into<primitives::v1::AccountId>,
	<R as fabric_system::Config>::Event: From<noble_balances::RawEvent<
		<R as fabric_system::Config>::AccountId,
		<R as noble_balances::Config>::Balance,
		noble_balances::DefaultInstance>
	>,
{
	fn on_nonzero_unbalanced(amount: NegativeImbalance<R>) {
		let numeric_amount = amount.peek();
		let author = <noble_authorship::Module<R>>::author();
		<noble_balances::Module<R>>::resolve_creating(&<noble_authorship::Module<R>>::author(), amount);
		<fabric_system::Module<R>>::deposit_event(noble_balances::RawEvent::Deposit(author, numeric_amount));
	}
}

pub struct DealWithFees<R>(tetcore_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
	R: noble_balances::Config + noble_treasury::Config + noble_authorship::Config,
	noble_treasury::Module<R>: OnUnbalanced<NegativeImbalance<R>>,
	<R as fabric_system::Config>::AccountId: From<primitives::v1::AccountId>,
	<R as fabric_system::Config>::AccountId: Into<primitives::v1::AccountId>,
	<R as fabric_system::Config>::Event: From<noble_balances::RawEvent<
		<R as fabric_system::Config>::AccountId,
		<R as noble_balances::Config>::Balance,
		noble_balances::DefaultInstance>
	>,
{
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item=NegativeImbalance<R>>) {
		if let Some(fees) = fees_then_tips.next() {
			// for fees, 80% to treasury, 20% to author
			let mut split = fees.ration(80, 20);
			if let Some(tips) = fees_then_tips.next() {
				// for tips, if any, 100% to author
				tips.merge_into(&mut split.1);
			}
			use noble_treasury::Module as Treasury;
			<Treasury<R> as OnUnbalanced<_>>::on_unbalanced(split.0);
			<ToAuthor<R> as OnUnbalanced<_>>::on_unbalanced(split.1);
		}
	}
}


#[cfg(test)]
mod tests {
	use super::*;
	use fabric_system::limits;
	use fabric_support::{impl_outer_origin, parameter_types, weights::DispatchClass};
	use fabric_support::traits::FindAuthor;
	use tet_core::H256;
	use tp_runtime::{
		testing::Header, ModuleId,
		traits::{BlakeTwo256, IdentityLookup},
		Perbill,
	};
	use primitives::v1::AccountId;

	#[derive(Clone, PartialEq, Eq, Debug)]
	pub struct Test;

	impl_outer_origin!{
		pub enum Origin for Test {}
	}

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub BlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
			.for_class(DispatchClass::all(), |weight| {
				weight.base_extrinsic = 100;
			})
			.for_class(DispatchClass::non_mandatory(), |weight| {
				weight.max_total = Some(1024);
			})
			.build_or_panic();
		pub BlockLength: limits::BlockLength = limits::BlockLength::max(2 * 1024);
		pub const AvailableBlockRatio: Perbill = Perbill::one();
	}

	impl fabric_system::Config for Test {
		type BaseCallFilter = ();
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Call = ();
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type BlockLength = BlockLength;
		type BlockWeights = BlockWeights;
		type DbWeight = ();
		type Version = ();
		type PalletInfo = ();
		type AccountData = noble_balances::AccountData<u64>;
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
	}

	impl noble_balances::Config for Test {
		type Balance = u64;
		type Event = ();
		type DustRemoval = ();
		type ExistentialDeposit = ();
		type AccountStore = System;
		type MaxLocks = ();
		type WeightInfo = ();
	}

	parameter_types! {
		pub const TreasuryModuleId: ModuleId = ModuleId(*b"py/trsry");
	}

	impl noble_treasury::Config for Test {
		type Currency = noble_balances::Module<Test>;
		type ApproveOrigin = fabric_system::EnsureRoot<AccountId>;
		type RejectOrigin = fabric_system::EnsureRoot<AccountId>;
		type Event = ();
		type OnSlash = ();
		type ProposalBond = ();
		type ProposalBondMinimum = ();
		type SpendPeriod = ();
		type Burn = ();
		type BurnDestination = ();
		type ModuleId = TreasuryModuleId;
		type SpendFunds = ();
		type WeightInfo = ();
	}

	pub struct OneAuthor;
	impl FindAuthor<AccountId> for OneAuthor {
		fn find_author<'a, I>(_: I) -> Option<AccountId>
			where I: 'a,
		{
			Some(Default::default())
		}
	}
	impl noble_authorship::Config for Test {
		type FindAuthor = OneAuthor;
		type UncleGenerations = ();
		type FilterUncle = ();
		type EventHandler = ();
	}

	type Treasury = noble_treasury::Module<Test>;
	type Balances = noble_balances::Module<Test>;
	type System = fabric_system::Module<Test>;

	pub fn new_test_ext() -> tet_io::TestExternalities {
		let mut t = fabric_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		// We use default for brevity, but you can configure as desired if needed.
		noble_balances::GenesisConfig::<Test>::default().assimilate_storage(&mut t).unwrap();
		t.into()
	}

	#[test]
	fn test_fees_and_tip_split() {
		new_test_ext().execute_with(|| {
			let fee = Balances::issue(10);
			let tip = Balances::issue(20);

			assert_eq!(Balances::free_balance(Treasury::account_id()), 0);
			assert_eq!(Balances::free_balance(AccountId::default()), 0);

			DealWithFees::on_unbalanceds(vec![fee, tip].into_iter());

			// Author gets 100% of tip and 20% of fee = 22
			assert_eq!(Balances::free_balance(AccountId::default()), 22);
			// Treasury gets 80% of fee
			assert_eq!(Balances::free_balance(Treasury::account_id()), 8);
		});
	}
}
