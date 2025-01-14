// Copyright 2019-2022 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

//! Test utilities
use crate as pallet_parachain_staking;
use crate::{pallet, AwardedPts, Config, Points, COLLATOR_LOCK_ID, NOMINATOR_LOCK_ID};
use frame_support::{
    assert_ok, construct_runtime, parameter_types,
    traits::{
        ConstU8, Currency, Everything, FindAuthor, GenesisBuild, Imbalance, LockIdentifier,
        OnFinalize, OnInitialize, OnUnbalanced,
    },
    weights::{DispatchClass, DispatchInfo, PostDispatchInfo, Weight, WeightToFee as WeightToFeeT},
    PalletId,
};
use frame_system::limits;
use pallet_transaction_payment::{ChargeTransactionPayment, CurrencyAdapter};
use sp_core::H256;
use sp_io;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup, SignedExtension},
    Perbill, SaturatedConversion,
};

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        ParachainStaking: pallet_parachain_staking::{Pallet, Call, Storage, Config<T>, Event<T>},
        Authorship: pallet_authorship::{Pallet, Call, Storage, Inherent},
        TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>, Config},
    }
);

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const MAX_BLOCK_WEIGHT: Weight = 1024;
pub static TX_LEN: usize = 1;
pub const BASE_FEE: u64 = 12;

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::one();
    pub const SS58Prefix: u8 = 42;

    pub BlockLength: limits::BlockLength = limits::BlockLength::max_with_normal_ratio(1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
        .base_block(10)
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = BASE_FEE;
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAX_BLOCK_WEIGHT);
            weights.reserved = Some(
                MAX_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT
            );
    })
    .avg_block_initialization(Perbill::from_percent(0))
    .build_or_panic();
}
impl frame_system::Config for Test {
    type BaseCallFilter = Everything;
    type DbWeight = ();
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = BlockNumber;
    type Call = Call;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = BlockLength;
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}
parameter_types! {
    pub const ExistentialDeposit: u128 = 0;
}
impl pallet_balances::Config for Test {
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 4];
    type MaxLocks = ();
    type Balance = Balance;
    type Event = Event;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

pub struct Author4;
impl FindAuthor<u64> for Author4 {
    fn find_author<'a, I>(_digests: I) -> Option<u64>
    where
        I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
    {
        Some(4)
    }
}

impl pallet_authorship::Config for Test {
    type FindAuthor = Author4;
    type UncleGenerations = ();
    type FilterUncle = ();
    type EventHandler = ParachainStaking;
}

parameter_types! {
    pub const MinBlocksPerEra: u32 = 3;
    pub const DefaultBlocksPerEra: u32 = 5;
    pub const LeaveCandidatesDelay: u32 = 2;
    pub const CandidateBondLessDelay: u32 = 2;
    pub const LeaveNominatorsDelay: u32 = 2;
    pub const RevokeNominationDelay: u32 = 2;
    pub const NominationBondLessDelay: u32 = 2;
    pub const RewardPaymentDelay: u32 = 2;
    pub const MinSelectedCandidates: u32 = 5;
    pub const MaxTopNominationsPerCandidate: u32 = 4;
    pub const MaxBottomNominationsPerCandidate: u32 = 4;
    pub const MaxNominationsPerNominator: u32 = 4;
    pub const MinCollatorStk: u128 = 10;
    pub const MinNominatorStk: u128 = 5;
    pub const MinNomination: u128 = 3;
    pub const RewardPotId: PalletId = PalletId(*b"av/vamgr");
}
impl Config for Test {
    type Event = Event;
    type Currency = Balances;
    type MonetaryGovernanceOrigin = frame_system::EnsureRoot<AccountId>;
    type MinBlocksPerEra = MinBlocksPerEra;
    type DefaultBlocksPerEra = DefaultBlocksPerEra;
    type LeaveCandidatesDelay = LeaveCandidatesDelay;
    type CandidateBondLessDelay = CandidateBondLessDelay;
    type LeaveNominatorsDelay = LeaveNominatorsDelay;
    type RevokeNominationDelay = RevokeNominationDelay;
    type NominationBondLessDelay = NominationBondLessDelay;
    type RewardPaymentDelay = RewardPaymentDelay;
    type MinSelectedCandidates = MinSelectedCandidates;
    type MaxTopNominationsPerCandidate = MaxTopNominationsPerCandidate;
    type MaxBottomNominationsPerCandidate = MaxBottomNominationsPerCandidate;
    type MaxNominationsPerNominator = MaxNominationsPerNominator;
    type MinCollatorStk = MinCollatorStk;
    type MinCandidateStk = MinCollatorStk;
    type MinNominatorStk = MinNominatorStk;
    type MinNomination = MinNomination;
    type RewardPotId = RewardPotId;
    type OnCollatorPayout = ();
    type OnNewEra = ();
    type WeightInfo = ();
}

parameter_types! {
    pub static WeightToFee: u128 = 1u128;
    pub static TransactionByteFee: u128 = 0u128;
}

pub struct DealWithFees;
impl OnUnbalanced<pallet_balances::NegativeImbalance<Test>> for DealWithFees {
    fn on_unbalanceds<B>(
        mut fees_then_tips: impl Iterator<Item = pallet_balances::NegativeImbalance<Test>>,
    ) {
        if let Some(mut fees) = fees_then_tips.next() {
            if let Some(tips) = fees_then_tips.next() {
                tips.merge_into(&mut fees);
            }
            let staking_pot = ParachainStaking::compute_reward_pot_account_id();
            Balances::resolve_creating(&staking_pot, fees);
        }
    }
}

impl pallet_transaction_payment::Config for Test {
    type Event = Event;
    type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees>;
    type LengthToFee = TransactionByteFee;
    type WeightToFee = WeightToFee;
    type FeeMultiplierUpdate = ();
    type OperationalFeeMultiplier = ConstU8<5>;
}

impl WeightToFeeT for WeightToFee {
    type Balance = u128;

    fn weight_to_fee(weight: &Weight) -> Self::Balance {
        Self::Balance::saturated_from(*weight).saturating_mul(WEIGHT_TO_FEE.with(|v| *v.borrow()))
    }
}

impl WeightToFeeT for TransactionByteFee {
    type Balance = u128;

    fn weight_to_fee(weight: &Weight) -> Self::Balance {
        Self::Balance::saturated_from(*weight)
            .saturating_mul(TRANSACTION_BYTE_FEE.with(|v| *v.borrow()))
    }
}

pub(crate) struct ExtBuilder {
    // endowed accounts with balances
    balances: Vec<(AccountId, Balance)>,
    // [collator, amount]
    collators: Vec<(AccountId, Balance)>,
    // [nominator, collator, nomination_amount]
    nominations: Vec<(AccountId, AccountId, Balance)>,
}

impl Default for ExtBuilder {
    fn default() -> ExtBuilder {
        ExtBuilder { balances: vec![], nominations: vec![], collators: vec![] }
    }
}

impl ExtBuilder {
    pub(crate) fn with_balances(mut self, balances: Vec<(AccountId, Balance)>) -> Self {
        self.balances = balances;
        self
    }

    pub(crate) fn with_candidates(mut self, collators: Vec<(AccountId, Balance)>) -> Self {
        self.collators = collators;
        self
    }

    pub(crate) fn with_nominations(
        mut self,
        nominations: Vec<(AccountId, AccountId, Balance)>,
    ) -> Self {
        self.nominations = nominations;
        self
    }

    pub(crate) fn build(self) -> sp_io::TestExternalities {
        let mut t = frame_system::GenesisConfig::default()
            .build_storage::<Test>()
            .expect("Frame system builds valid default genesis config");

        pallet_balances::GenesisConfig::<Test> { balances: self.balances }
            .assimilate_storage(&mut t)
            .expect("Pallet balances storage can be assimilated");
        pallet_parachain_staking::GenesisConfig::<Test> {
            candidates: self.collators,
            nominations: self.nominations,
        }
        .assimilate_storage(&mut t)
        .expect("Parachain Staking's storage can be assimilated");

        let mut ext = sp_io::TestExternalities::new(t);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}

/// Rolls forward one block. Returns the new block number.
pub(crate) fn roll_one_block() -> u64 {
    Balances::on_finalize(System::block_number());
    System::on_finalize(System::block_number());
    System::set_block_number(System::block_number() + 1);
    System::on_initialize(System::block_number());
    Balances::on_initialize(System::block_number());
    ParachainStaking::on_initialize(System::block_number());
    System::block_number()
}

/// Rolls to the desired block. Returns the number of blocks played.
pub(crate) fn roll_to(n: u64) -> u64 {
    let mut num_blocks = 0;
    let mut block = System::block_number();
    while block < n {
        block = roll_one_block();
        num_blocks += 1;
    }
    num_blocks
}

/// Rolls block-by-block to the beginning of the specified era.
/// This will complete the block in which the era change occurs.
/// Returns the number of blocks played.
pub(crate) fn roll_to_era_begin(era: u64) -> u64 {
    let block = (era - 1) * DefaultBlocksPerEra::get() as u64;
    roll_to(block)
}

/// Rolls block-by-block to the end of the specified era.
/// The block following will be the one in which the specified era change occurs.
pub(crate) fn roll_to_era_end(era: u64) -> u64 {
    let block = era * DefaultBlocksPerEra::get() as u64 - 1;
    roll_to(block)
}

pub(crate) fn last_event() -> Event {
    System::events().pop().expect("Event expected").event
}

pub(crate) fn set_reward_pot(amount: Balance) {
    Balances::make_free_balance_be(&ParachainStaking::compute_reward_pot_account_id(), amount);
    crate::LockedEraPayout::<Test>::put(0);
}

pub(crate) fn events() -> Vec<pallet::Event<Test>> {
    System::events()
        .into_iter()
        .map(|r| r.event)
        .filter_map(|e| if let Event::ParachainStaking(inner) = e { Some(inner) } else { None })
        .collect::<Vec<_>>()
}

/// Assert input equal to the last event emitted
#[macro_export]
macro_rules! assert_last_event {
    ($event:expr) => {
        match &$event {
            e => assert_eq!(*e, crate::mock::last_event()),
        }
    };
}

/// Compares the system events with passed in events
/// Prints highlighted diff iff assert_eq fails
#[macro_export]
macro_rules! assert_eq_events {
    ($events:expr) => {
        match &$events {
            e => similar_asserts::assert_eq!(*e, crate::mock::events()),
        }
    };
}

/// Compares the last N system events with passed in events, where N is the length of events passed
/// in.
///
/// Prints highlighted diff iff assert_eq fails.
/// The last events from frame_system will be taken in order to match the number passed to this
/// macro. If there are insufficient events from frame_system, they will still be compared; the
/// output may or may not be helpful.
///
/// Examples:
/// If frame_system has events [A, B, C, D, E] and events [C, D, E] are passed in, the result would
/// be a successful match ([C, D, E] == [C, D, E]).
///
/// If frame_system has events [A, B, C, D] and events [B, C] are passed in, the result would be an
/// error and a hopefully-useful diff will be printed between [C, D] and [B, C].
///
/// Note that events are filtered to only match parachain-staking (see events()).
#[macro_export]
macro_rules! assert_eq_last_events {
	($events:expr $(,)?) => {
		assert_tail_eq!($events, crate::mock::events());
	};
	($events:expr, $($arg:tt)*) => {
		assert_tail_eq!($events, crate::mock::events(), $($arg)*);
	};
}

/// Assert that one array is equal to the tail of the other. A more generic and testable version of
/// assert_eq_last_events.
#[macro_export]
macro_rules! assert_tail_eq {
	($tail:expr, $arr:expr $(,)?) => {
		if $tail.len() != 0 {
			// 0-length always passes

			if $tail.len() > $arr.len() {
				similar_asserts::assert_eq!($tail, $arr); // will fail
			}

			let len_diff = $arr.len() - $tail.len();
			similar_asserts::assert_eq!($tail, $arr[len_diff..]);
		}
	};
	($tail:expr, $arr:expr, $($arg:tt)*) => {
		if $tail.len() != 0 {
			// 0-length always passes

			if $tail.len() > $arr.len() {
				similar_asserts::assert_eq!($tail, $arr, $($arg)*); // will fail
			}

			let len_diff = $arr.len() - $tail.len();
			similar_asserts::assert_eq!($tail, $arr[len_diff..], $($arg)*);
		}
	};
}

/// Panics if an event is not found in the system log of events
#[macro_export]
macro_rules! assert_event_emitted {
    ($event:expr) => {
        match &$event {
            e => {
                assert!(
                    crate::mock::events().iter().find(|x| *x == e).is_some(),
                    "Event {:?} was not found in events: \n {:?}",
                    e,
                    crate::mock::events()
                );
            },
        }
    };
}

/// Panics if an event is found in the system log of events
#[macro_export]
macro_rules! assert_event_not_emitted {
    ($event:expr) => {
        match &$event {
            e => {
                assert!(
                    crate::mock::events().iter().find(|x| *x == e).is_none(),
                    "Event {:?} was found in events: \n {:?}",
                    e,
                    crate::mock::events()
                );
            },
        }
    };
}

// Same storage changes as ParachainStaking::on_finalize
pub(crate) fn set_author(era: u32, acc: u64, pts: u32) {
    <Points<Test>>::mutate(era, |p| *p += pts);
    <AwardedPts<Test>>::mutate(era, acc, |p| *p += pts);
}

/// fn to query the lock amount
pub(crate) fn query_lock_amount(account_id: u64, id: LockIdentifier) -> Option<Balance> {
    for lock in Balances::locks(&account_id) {
        if lock.id == id {
            return Some(lock.amount)
        }
    }
    None
}

pub(crate) fn pay_gas_for_transaction(sender: &AccountId, tip: u128) {
    let pre = ChargeTransactionPayment::<Test>::from(tip)
        .pre_dispatch(
            sender,
            &Call::System(frame_system::Call::remark { remark: vec![] }),
            &DispatchInfo { weight: 1, ..Default::default() },
            TX_LEN,
        )
        .unwrap();

    assert_ok!(ChargeTransactionPayment::<Test>::post_dispatch(
        Some(pre),
        &DispatchInfo { weight: 1, ..Default::default() },
        &PostDispatchInfo { actual_weight: None, pays_fee: Default::default() },
        TX_LEN,
        &Ok(())
    ));
}

#[test]
fn geneses() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 1000),
            (2, 300),
            (3, 100),
            (4, 100),
            (5, 100),
            (6, 100),
            (7, 100),
            (8, 9),
            (9, 4),
        ])
        .with_candidates(vec![(1, 500), (2, 200)])
        .with_nominations(vec![(3, 1, 100), (4, 1, 100), (5, 2, 100), (6, 2, 100)])
        .build()
        .execute_with(|| {
            assert!(System::events().is_empty());
            // collators
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 500);
            assert_eq!(query_lock_amount(1, COLLATOR_LOCK_ID), Some(500));
            assert!(ParachainStaking::is_candidate(&1));
            assert_eq!(query_lock_amount(2, COLLATOR_LOCK_ID), Some(200));
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&2), 100);
            assert!(ParachainStaking::is_candidate(&2));
            // nominators
            for x in 3..7 {
                assert!(ParachainStaking::is_nominator(&x));
                assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&x), 0);
                assert_eq!(query_lock_amount(x, NOMINATOR_LOCK_ID), Some(100));
            }
            // uninvolved
            for x in 7..10 {
                assert!(!ParachainStaking::is_nominator(&x));
            }
            // no nominator staking locks
            assert_eq!(query_lock_amount(7, NOMINATOR_LOCK_ID), None);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&7), 100);
            assert_eq!(query_lock_amount(8, NOMINATOR_LOCK_ID), None);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&8), 9);
            assert_eq!(query_lock_amount(9, NOMINATOR_LOCK_ID), None);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&9), 4);
            // no collator staking locks
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&7), 100);
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&8), 9);
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&9), 4);
        });
    ExtBuilder::default()
        .with_balances(vec![
            (1, 100),
            (2, 100),
            (3, 100),
            (4, 100),
            (5, 100),
            (6, 100),
            (7, 100),
            (8, 100),
            (9, 100),
            (10, 100),
        ])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 10)])
        .with_nominations(vec![(6, 1, 10), (7, 1, 10), (8, 2, 10), (9, 2, 10), (10, 1, 10)])
        .build()
        .execute_with(|| {
            assert!(System::events().is_empty());
            // collators
            for x in 1..5 {
                assert!(ParachainStaking::is_candidate(&x));
                assert_eq!(query_lock_amount(x, COLLATOR_LOCK_ID), Some(20));
                assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&x), 80);
            }
            assert!(ParachainStaking::is_candidate(&5));
            assert_eq!(query_lock_amount(5, COLLATOR_LOCK_ID), Some(10));
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&5), 90);
            // nominators
            for x in 6..11 {
                assert!(ParachainStaking::is_nominator(&x));
                assert_eq!(query_lock_amount(x, NOMINATOR_LOCK_ID), Some(10));
                assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&x), 90);
            }
        });
}

#[test]
fn roll_to_era_begin_works() {
    ExtBuilder::default().build().execute_with(|| {
        // these tests assume blocks-per-era of 5, as established by DefaultBlocksPerEra
        assert_eq!(System::block_number(), 1); // we start on block 1

        let num_blocks = roll_to_era_begin(1);
        assert_eq!(System::block_number(), 1); // no-op, we're already on this era
        assert_eq!(num_blocks, 0);

        let num_blocks = roll_to_era_begin(2);
        assert_eq!(System::block_number(), 5);
        assert_eq!(num_blocks, 4);

        let num_blocks = roll_to_era_begin(3);
        assert_eq!(System::block_number(), 10);
        assert_eq!(num_blocks, 5);
    });
}

#[test]
fn roll_to_era_end_works() {
    ExtBuilder::default().build().execute_with(|| {
        // these tests assume blocks-per-era of 5, as established by DefaultBlocksPerEra
        assert_eq!(System::block_number(), 1); // we start on block 1

        let num_blocks = roll_to_era_end(1);
        assert_eq!(System::block_number(), 4);
        assert_eq!(num_blocks, 3);

        let num_blocks = roll_to_era_end(2);
        assert_eq!(System::block_number(), 9);
        assert_eq!(num_blocks, 5);

        let num_blocks = roll_to_era_end(3);
        assert_eq!(System::block_number(), 14);
        assert_eq!(num_blocks, 5);
    });
}

#[test]
fn assert_tail_eq_works() {
    assert_tail_eq!(vec![1, 2], vec![0, 1, 2]);

    assert_tail_eq!(vec![1], vec![1]);

    assert_tail_eq!(
        vec![0u32; 0], // 0 length array
        vec![0u32; 1]  // 1-length array
    );

    assert_tail_eq!(vec![0u32, 0], vec![0u32, 0]);
}

#[test]
#[should_panic]
fn assert_tail_eq_panics_on_non_equal_tail() {
    assert_tail_eq!(vec![2, 2], vec![0, 1, 2]);
}

#[test]
#[should_panic]
fn assert_tail_eq_panics_on_empty_arr() {
    assert_tail_eq!(vec![2, 2], vec![0u32; 0]);
}

#[test]
#[should_panic]
fn assert_tail_eq_panics_on_longer_tail() {
    assert_tail_eq!(vec![1, 2, 3], vec![1, 2]);
}

#[test]
#[should_panic]
fn assert_tail_eq_panics_on_unequal_elements_same_length_array() {
    assert_tail_eq!(vec![1, 2, 3], vec![0, 1, 2]);
}
