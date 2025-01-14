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

//! # Parachain Staking
//! Minimal staking pallet that implements collator selection by total backed stake.
//! The main difference between this pallet and `frame/pallet-staking` is that this pallet
//! uses direct nomination. Nominators choose exactly who they nominate and with what stake.
//! This is different from `frame/pallet-staking` where nominators approval vote and run Phragmen.
//!
//! ### Rules
//! There is a new era every `<Era<T>>::get().length` blocks.
//!
//! At the start of every era,
//! * issuance is calculated for collators (and their nominators) for block authoring
//! `T::RewardPaymentDelay` eras ago
//! * a new set of collators is chosen from the candidates
//!
//! Immediately following a era change, payments are made once-per-block until all payments have
//! been made. In each such block, one collator is chosen for a rewards payment and is paid along
//! with each of its top `T::MaxTopNominationsPerCandidate` nominators.
//!
//! To join the set of candidates, call `join_candidates` with `bond >= MinCandidateStk`.
//! To leave the set of candidates, call `schedule_leave_candidates`. If the call succeeds,
//! the collator is removed from the pool of candidates so they cannot be selected for future
//! collator sets, but they are not unbonded until their exit request is executed. Any signed
//! account may trigger the exit `T::LeaveCandidatesDelay` eras after the era in which the
//! original request was made.
//!
//! To join the set of nominators, call `nominate` and pass in an account that is
//! already a collator candidate and `bond >= MinNominatorStk`. Each nominator can nominate up to
//! `T::MaxNominationsPerNominator` collator candidates by calling `nominate`.
//!
//! To revoke a nomination, call `revoke_nomination` with the collator candidate's account.
//! To leave the set of nominators and revoke all nominations, call `leave_nominators`.

#![cfg_attr(not(feature = "std"), no_std)]

mod nomination_requests;
pub mod traits;
pub mod types;
pub mod weights;

#[cfg(any(test, feature = "runtime-benchmarks"))]
mod benchmarks;
#[cfg(test)]
mod mock;
mod set;
#[cfg(test)]
mod test_reward_payout;
#[cfg(test)]
mod test_staking_pot;
#[cfg(test)]
mod tests;

use frame_support::pallet;
use weights::WeightInfo;

pub use nomination_requests::{CancelledScheduledRequest, NominationAction, ScheduledRequest};
pub use pallet::*;
pub use traits::*;
pub use types::*;
pub use EraIndex;

#[pallet]
pub mod pallet {
    use crate::{
        nomination_requests::{CancelledScheduledRequest, NominationAction, ScheduledRequest},
        set::OrderedSet,
        traits::*,
        types::*,
        WeightInfo,
    };
    use frame_support::{
        pallet_prelude::*,
        traits::{
            tokens::WithdrawReasons, Currency, ExistenceRequirement, Get, LockIdentifier,
            LockableCurrency, ReservableCurrency,
        },
        PalletId,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::{
        traits::{AccountIdConversion, Bounded, CheckedAdd, CheckedSub, Saturating, Zero},
        Perbill,
    };
    use sp_std::{collections::btree_map::BTreeMap, prelude::*};

    /// Pallet for parachain staking
    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    pub type EraIndex = u32;
    type RewardPoint = u32;
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    pub const COLLATOR_LOCK_ID: LockIdentifier = *b"stkngcol";
    pub const NOMINATOR_LOCK_ID: LockIdentifier = *b"stkngdel";

    /// Configuration trait of this pallet.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Overarching event type
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
        /// The currency type
        type Currency: Currency<Self::AccountId>
            + ReservableCurrency<Self::AccountId>
            + LockableCurrency<Self::AccountId>;
        /// The origin for monetary governance
        type MonetaryGovernanceOrigin: EnsureOrigin<Self::Origin>;
        /// Minimum number of blocks per era
        #[pallet::constant]
        type MinBlocksPerEra: Get<u32>;
        /// Default number of blocks per era at genesis
        #[pallet::constant]
        type DefaultBlocksPerEra: Get<u32>;
        /// Number of eras that candidates remain bonded before exit request is executable
        #[pallet::constant]
        type LeaveCandidatesDelay: Get<EraIndex>;
        /// Number of eras candidate requests to decrease self-bond must wait to be executable
        #[pallet::constant]
        type CandidateBondLessDelay: Get<EraIndex>;
        /// Number of eras that nominators remain bonded before exit request is executable
        #[pallet::constant]
        type LeaveNominatorsDelay: Get<EraIndex>;
        /// Number of eras that nominations remain bonded before revocation request is executable
        #[pallet::constant]
        type RevokeNominationDelay: Get<EraIndex>;
        /// Number of eras that nomination less requests must wait before executable
        #[pallet::constant]
        type NominationBondLessDelay: Get<EraIndex>;
        /// Number of eras after which block authors are rewarded
        #[pallet::constant]
        type RewardPaymentDelay: Get<EraIndex>;
        /// Minimum number of selected candidates every era
        #[pallet::constant]
        type MinSelectedCandidates: Get<u32>;
        /// Maximum top nominations counted per candidate
        #[pallet::constant]
        type MaxTopNominationsPerCandidate: Get<u32>;
        /// Maximum bottom nominations (not counted) per candidate
        #[pallet::constant]
        type MaxBottomNominationsPerCandidate: Get<u32>;
        /// Maximum nominations per nominator
        #[pallet::constant]
        type MaxNominationsPerNominator: Get<u32>;
        /// Minimum stake required for any candidate to be in `SelectedCandidates` for the era
        #[pallet::constant]
        type MinCollatorStk: Get<BalanceOf<Self>>;
        /// Minimum stake required for any account to be a collator candidate
        #[pallet::constant]
        type MinCandidateStk: Get<BalanceOf<Self>>;
        /// Minimum stake for any registered on-chain account to nominate
        #[pallet::constant]
        type MinNomination: Get<BalanceOf<Self>>;
        /// Minimum stake for any registered on-chain account to be a nominator
        #[pallet::constant]
        type MinNominatorStk: Get<BalanceOf<Self>>;
        /// Id of the account that will hold funds to be paid as staking reward
        type RewardPotId: Get<PalletId>;
        /// Handler to notify the runtime when a collator is paid.
        /// If you don't need it, you can specify the type `()`.
        type OnCollatorPayout: OnCollatorPayout<Self::AccountId, BalanceOf<Self>>;
        /// Handler to notify the runtime when a new era begin.
        /// If you don't need it, you can specify the type `()`.
        type OnNewEra: OnNewEra;
        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::error]
    pub enum Error<T> {
        NominatorDNE,
        NominatorDNEinTopNorBottom,
        NominatorDNEInNominatorSet,
        CandidateDNE,
        NominationDNE,
        NominatorExists,
        CandidateExists,
        CandidateBondBelowMin,
        InsufficientBalance,
        NominatorBondBelowMin,
        NominationBelowMin,
        AlreadyOffline,
        AlreadyActive,
        NominatorAlreadyLeaving,
        NominatorNotLeaving,
        NominatorCannotLeaveYet,
        CannotNominateIfLeaving,
        CandidateAlreadyLeaving,
        CandidateNotLeaving,
        CandidateCannotLeaveYet,
        CannotGoOnlineIfLeaving,
        ExceedMaxNominationsPerNominator,
        AlreadyNominatedCandidate,
        InvalidSchedule,
        CannotSetBelowMin,
        EraLengthMustBeAtLeastTotalSelectedCollators,
        NoWritingSameValue,
        TooLowCandidateCountWeightHintJoinCandidates,
        TooLowCandidateCountWeightHintCancelLeaveCandidates,
        TooLowCandidateCountToLeaveCandidates,
        TooLowNominationCountToNominate,
        TooLowCandidateNominationCountToNominate,
        TooLowCandidateNominationCountToLeaveCandidates,
        TooLowNominationCountToLeaveNominators,
        PendingCandidateRequestsDNE,
        PendingCandidateRequestAlreadyExists,
        PendingCandidateRequestNotDueYet,
        PendingNominationRequestDNE,
        PendingNominationRequestAlreadyExists,
        PendingNominationRequestNotDueYet,
        CannotNominateLessThanOrEqualToLowestBottomWhenFull,
        PendingNominationRevoke,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Started new era.
        NewEra {
            starting_block: T::BlockNumber,
            era: EraIndex,
            selected_collators_number: u32,
            total_balance: BalanceOf<T>,
        },
        /// Account joined the set of collator candidates.
        JoinedCollatorCandidates {
            account: T::AccountId,
            amount_locked: BalanceOf<T>,
            new_total_amt_locked: BalanceOf<T>,
        },
        /// Candidate selected for collators. Total Exposed Amount includes all nominations.
        CollatorChosen {
            era: EraIndex,
            collator_account: T::AccountId,
            total_exposed_amount: BalanceOf<T>,
        },
        /// Candidate requested to decrease a self bond.
        CandidateBondLessRequested {
            candidate: T::AccountId,
            amount_to_decrease: BalanceOf<T>,
            execute_era: EraIndex,
        },
        /// Candidate has increased a self bond.
        CandidateBondedMore {
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            new_total_bond: BalanceOf<T>,
        },
        /// Candidate has decreased a self bond.
        CandidateBondedLess {
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            new_bond: BalanceOf<T>,
        },
        /// Candidate temporarily leave the set of collator candidates without unbonding.
        CandidateWentOffline { candidate: T::AccountId },
        /// Candidate rejoins the set of collator candidates.
        CandidateBackOnline { candidate: T::AccountId },
        /// Candidate has requested to leave the set of candidates.
        CandidateScheduledExit {
            exit_allowed_era: EraIndex,
            candidate: T::AccountId,
            scheduled_exit: EraIndex,
        },
        /// Cancelled request to leave the set of candidates.
        CancelledCandidateExit { candidate: T::AccountId },
        /// Cancelled request to decrease candidate's bond.
        CancelledCandidateBondLess {
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            execute_era: EraIndex,
        },
        /// Candidate has left the set of candidates.
        CandidateLeft {
            ex_candidate: T::AccountId,
            unlocked_amount: BalanceOf<T>,
            new_total_amt_locked: BalanceOf<T>,
        },
        /// Nominator requested to decrease a bond for the collator candidate.
        NominationDecreaseScheduled {
            nominator: T::AccountId,
            candidate: T::AccountId,
            amount_to_decrease: BalanceOf<T>,
            execute_era: EraIndex,
        },
        // Nomination increased.
        NominationIncreased {
            nominator: T::AccountId,
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            in_top: bool,
        },
        // Nomination decreased.
        NominationDecreased {
            nominator: T::AccountId,
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            in_top: bool,
        },
        /// Nominator requested to leave the set of nominators.
        NominatorExitScheduled { era: EraIndex, nominator: T::AccountId, scheduled_exit: EraIndex },
        /// Nominator requested to revoke nomination.
        NominationRevocationScheduled {
            era: EraIndex,
            nominator: T::AccountId,
            candidate: T::AccountId,
            scheduled_exit: EraIndex,
        },
        /// Nominator has left the set of nominators.
        NominatorLeft { nominator: T::AccountId, unstaked_amount: BalanceOf<T> },
        /// Nomination revoked.
        NominationRevoked {
            nominator: T::AccountId,
            candidate: T::AccountId,
            unstaked_amount: BalanceOf<T>,
        },
        /// Nomination kicked.
        NominationKicked {
            nominator: T::AccountId,
            candidate: T::AccountId,
            unstaked_amount: BalanceOf<T>,
        },
        /// Cancelled a pending request to exit the set of nominators.
        NominatorExitCancelled { nominator: T::AccountId },
        /// Cancelled request to change an existing nomination.
        CancelledNominationRequest {
            nominator: T::AccountId,
            cancelled_request: CancelledScheduledRequest<BalanceOf<T>>,
            collator: T::AccountId,
        },
        /// New nomination (increase of the existing one).
        Nomination {
            nominator: T::AccountId,
            locked_amount: BalanceOf<T>,
            candidate: T::AccountId,
            nominator_position: NominatorAdded<BalanceOf<T>>,
        },
        /// Nomination from candidate state has been remove.
        NominatorLeftCandidate {
            nominator: T::AccountId,
            candidate: T::AccountId,
            unstaked_amount: BalanceOf<T>,
            total_candidate_staked: BalanceOf<T>,
        },
        /// Paid the account (nominator or collator) the balance as liquid rewards.
        Rewarded { account: T::AccountId, rewards: BalanceOf<T> },
        /// There was an error attempting to pay the nominator their staking reward.
        ErrorPayingStakingReward { payee: T::AccountId, rewards: BalanceOf<T> },
        /// Set total selected candidates to this value.
        TotalSelectedSet { old: u32, new: u32 },
        /// Set blocks per era
        BlocksPerEraSet { current_era: EraIndex, first_block: T::BlockNumber, old: u32, new: u32 },
        /// Not enough fund to cover the staking reward payment.
        NotEnoughFundsForEraPayment { reward_pot_balance: BalanceOf<T> },
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: T::BlockNumber) -> Weight {
            let mut weight = T::WeightInfo::base_on_initialize();

            let mut era = <Era<T>>::get();
            if era.should_update(n) {
                // mutate era
                era.update(n);
                // notify that new era begin
                weight = weight.saturating_add(T::OnNewEra::on_new_era(era.current));
                // pay all stakers for T::RewardPaymentDelay eras ago
                Self::prepare_staking_payouts(era.current);
                // select top collator candidates for next era
                let (collator_count, nomination_count, total_staked) =
                    Self::select_top_candidates(era.current);
                // start next era
                <Era<T>>::put(era);
                // snapshot total stake
                <Staked<T>>::insert(era.current, <Total<T>>::get());
                Self::deposit_event(Event::NewEra {
                    starting_block: era.first,
                    era: era.current,
                    selected_collators_number: collator_count,
                    total_balance: total_staked,
                });
                weight = weight.saturating_add(T::WeightInfo::era_transition_on_initialize(
                    collator_count,
                    nomination_count,
                ));
            }

            weight = weight.saturating_add(Self::handle_delayed_payouts(era.current));

            // add on_finalize weight
            weight = weight.saturating_add(
                // read Author, Points, AwardedPts
                // write Points, AwardedPts
                T::DbWeight::get().reads(3).saturating_add(T::DbWeight::get().writes(2)),
            );
            weight
        }
    }

    #[pallet::storage]
    #[pallet::getter(fn total_selected)]
    /// The total candidates selected every era
    type TotalSelected<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn era)]
    /// Current era index and next era scheduled transition
    pub(crate) type Era<T: Config> = StorageValue<_, EraInfo<T::BlockNumber>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn nominator_state)]
    /// Get nominator state associated with an account if account is nominating else None
    pub(crate) type NominatorState<T: Config> = StorageMap<
        _,
        Twox64Concat,
        T::AccountId,
        Nominator<T::AccountId, BalanceOf<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn candidate_info)]
    /// Get collator candidate info associated with an account if account is candidate else None
    pub(crate) type CandidateInfo<T: Config> =
        StorageMap<_, Twox64Concat, T::AccountId, CandidateMetadata<BalanceOf<T>>, OptionQuery>;

    /// Stores outstanding nomination requests per collator.
    #[pallet::storage]
    #[pallet::getter(fn nomination_scheduled_requests)]
    pub(crate) type NominationScheduledRequests<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Vec<ScheduledRequest<T::AccountId, BalanceOf<T>>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn top_nominations)]
    /// Top nominations for collator candidate
    pub(crate) type TopNominations<T: Config> = StorageMap<
        _,
        Twox64Concat,
        T::AccountId,
        Nominations<T::AccountId, BalanceOf<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn bottom_nominations)]
    /// Bottom nominations for collator candidate
    pub(crate) type BottomNominations<T: Config> = StorageMap<
        _,
        Twox64Concat,
        T::AccountId,
        Nominations<T::AccountId, BalanceOf<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn selected_candidates)]
    /// The collator candidates selected for the current era
    type SelectedCandidates<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total)]
    /// Total capital locked by this staking pallet
    pub(crate) type Total<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn candidate_pool)]
    /// The pool of collator candidates, each with their total backing stake
    pub(crate) type CandidatePool<T: Config> =
        StorageValue<_, OrderedSet<Bond<T::AccountId, BalanceOf<T>>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn at_stake)]
    /// Snapshot of collator nomination stake at the start of the era
    pub type AtStake<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        EraIndex,
        Twox64Concat,
        T::AccountId,
        CollatorSnapshot<T::AccountId, BalanceOf<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn delayed_payouts)]
    /// Delayed payouts
    pub type DelayedPayouts<T: Config> =
        StorageMap<_, Twox64Concat, EraIndex, DelayedPayout<BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn staked)]
    /// Total counted stake for selected candidates in the era
    pub type Staked<T: Config> = StorageMap<_, Twox64Concat, EraIndex, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn points)]
    /// Total points awarded to collators for block production in the era
    pub type Points<T: Config> = StorageMap<_, Twox64Concat, EraIndex, RewardPoint, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn awarded_pts)]
    /// Points for each collator per era
    pub type AwardedPts<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        EraIndex,
        Twox64Concat,
        T::AccountId,
        RewardPoint,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn locked_era_payout)]
    /// Storage value that holds the total amount of payouts we are waiting to take out of this
    /// pallet's pot.
    pub type LockedEraPayout<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn faild_payments)]
    /// Storage value that holds any failed payments
    pub type FailedRewardPayments<T: Config> =
        StorageMap<_, Twox64Concat, BalanceOf<T>, bool, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub candidates: Vec<(T::AccountId, BalanceOf<T>)>,
        /// Vec of tuples of the format (nominator AccountId, collator AccountId, nomination
        /// Amount)
        pub nominations: Vec<(T::AccountId, T::AccountId, BalanceOf<T>)>,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { candidates: vec![], nominations: vec![] }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            let mut candidate_count = 0u32;
            // Initialize the candidates
            for &(ref candidate, balance) in &self.candidates {
                assert!(
                    <Pallet<T>>::get_collator_stakable_free_balance(candidate) >= balance,
                    "Account does not have enough balance to bond as a candidate."
                );
                candidate_count = candidate_count.saturating_add(1u32);
                if let Err(error) = <Pallet<T>>::join_candidates(
                    T::Origin::from(Some(candidate.clone()).into()),
                    balance,
                    candidate_count,
                ) {
                    log::warn!("Join candidates failed in genesis with error {:?}", error);
                } else {
                    candidate_count = candidate_count.saturating_add(1u32);
                }
            }
            let mut col_nominator_count: BTreeMap<T::AccountId, u32> = BTreeMap::new();
            let mut del_nomination_count: BTreeMap<T::AccountId, u32> = BTreeMap::new();
            // Initialize the nominations
            for &(ref nominator, ref target, balance) in &self.nominations {
                assert!(
                    <Pallet<T>>::get_nominator_stakable_free_balance(nominator) >= balance,
                    "Account does not have enough balance to place nomination."
                );
                let cd_count =
                    if let Some(x) = col_nominator_count.get(target) { *x } else { 0u32 };
                let dd_count =
                    if let Some(x) = del_nomination_count.get(nominator) { *x } else { 0u32 };
                if let Err(error) = <Pallet<T>>::nominate(
                    T::Origin::from(Some(nominator.clone()).into()),
                    target.clone(),
                    balance,
                    cd_count,
                    dd_count,
                ) {
                    log::warn!("Nominate failed in genesis with error {:?}", error);
                } else {
                    if let Some(x) = col_nominator_count.get_mut(target) {
                        *x = x.saturating_add(1u32);
                    } else {
                        col_nominator_count.insert(target.clone(), 1u32);
                    };
                    if let Some(x) = del_nomination_count.get_mut(nominator) {
                        *x = x.saturating_add(1u32);
                    } else {
                        del_nomination_count.insert(nominator.clone(), 1u32);
                    };
                }
            }
            // Set total selected candidates to minimum config
            <TotalSelected<T>>::put(T::MinSelectedCandidates::get());
            // Choose top TotalSelected collator candidates
            let (v_count, _, total_staked) = <Pallet<T>>::select_top_candidates(1u32);
            // Start Era 1 at Block 0
            let era: EraInfo<T::BlockNumber> =
                EraInfo::new(1u32, 0u32.into(), T::DefaultBlocksPerEra::get());
            <Era<T>>::put(era);
            // Snapshot total stake
            <Staked<T>>::insert(1u32, <Total<T>>::get());
            <Pallet<T>>::deposit_event(Event::NewEra {
                starting_block: T::BlockNumber::zero(),
                era: 1u32,
                selected_collators_number: v_count,
                total_balance: total_staked,
            });
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as Config>::WeightInfo::set_total_selected())]
        /// Set the total number of collator candidates selected per era
        /// - changes are not applied until the start of the next era
        pub fn set_total_selected(origin: OriginFor<T>, new: u32) -> DispatchResultWithPostInfo {
            frame_system::ensure_root(origin)?;
            ensure!(new >= T::MinSelectedCandidates::get(), Error::<T>::CannotSetBelowMin);
            let old = <TotalSelected<T>>::get();
            ensure!(old != new, Error::<T>::NoWritingSameValue);
            ensure!(
                new <= <Era<T>>::get().length,
                Error::<T>::EraLengthMustBeAtLeastTotalSelectedCollators,
            );
            <TotalSelected<T>>::put(new);
            Self::deposit_event(Event::TotalSelectedSet { old, new });
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::set_blocks_per_era())]
        /// Set blocks per era
        /// - if called with `new` less than length of current era, will transition immediately
        /// in the next block
        /// - also updates per-era inflation config
        pub fn set_blocks_per_era(origin: OriginFor<T>, new: u32) -> DispatchResultWithPostInfo {
            frame_system::ensure_root(origin)?;
            ensure!(new >= T::MinBlocksPerEra::get(), Error::<T>::CannotSetBelowMin);
            let mut era = <Era<T>>::get();
            let (now, first, old) = (era.current, era.first, era.length);
            ensure!(old != new, Error::<T>::NoWritingSameValue);
            ensure!(
                new >= <TotalSelected<T>>::get(),
                Error::<T>::EraLengthMustBeAtLeastTotalSelectedCollators,
            );
            era.length = new;
            <Era<T>>::put(era);
            Self::deposit_event(Event::BlocksPerEraSet {
                current_era: now,
                first_block: first,
                old,
                new,
            });

            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::join_candidates(*candidate_count))]
        /// Join the set of collator candidates
        pub fn join_candidates(
            origin: OriginFor<T>,
            bond: BalanceOf<T>,
            candidate_count: u32,
        ) -> DispatchResultWithPostInfo {
            let acc = ensure_signed(origin)?;
            ensure!(!Self::is_candidate(&acc), Error::<T>::CandidateExists);
            ensure!(!Self::is_nominator(&acc), Error::<T>::NominatorExists);
            ensure!(bond >= T::MinCandidateStk::get(), Error::<T>::CandidateBondBelowMin);
            let mut candidates = <CandidatePool<T>>::get();
            let old_count = candidates.0.len() as u32;
            ensure!(
                candidate_count >= old_count,
                Error::<T>::TooLowCandidateCountWeightHintJoinCandidates
            );
            ensure!(
                candidates.insert(Bond { owner: acc.clone(), amount: bond }),
                Error::<T>::CandidateExists
            );
            ensure!(
                Self::get_collator_stakable_free_balance(&acc) >= bond,
                Error::<T>::InsufficientBalance,
            );
            T::Currency::set_lock(COLLATOR_LOCK_ID, &acc, bond, WithdrawReasons::all());
            let candidate = CandidateMetadata::new(bond);
            <CandidateInfo<T>>::insert(&acc, candidate);
            let empty_nominations: Nominations<T::AccountId, BalanceOf<T>> = Default::default();
            // insert empty top nominations
            <TopNominations<T>>::insert(&acc, empty_nominations.clone());
            // insert empty bottom nominations
            <BottomNominations<T>>::insert(&acc, empty_nominations);
            <CandidatePool<T>>::put(candidates);
            let new_total = <Total<T>>::get().saturating_add(bond);
            <Total<T>>::put(new_total);
            Self::deposit_event(Event::JoinedCollatorCandidates {
                account: acc,
                amount_locked: bond,
                new_total_amt_locked: new_total,
            });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::schedule_leave_candidates(*candidate_count))]
        /// Request to leave the set of candidates. If successful, the account is immediately
        /// removed from the candidate pool to prevent selection as a collator.
        pub fn schedule_leave_candidates(
            origin: OriginFor<T>,
            candidate_count: u32,
        ) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            let (now, when) = state.schedule_leave::<T>()?;
            let mut candidates = <CandidatePool<T>>::get();
            ensure!(
                candidate_count >= candidates.0.len() as u32,
                Error::<T>::TooLowCandidateCountToLeaveCandidates
            );
            if candidates.remove(&Bond::from_owner(collator.clone())) {
                <CandidatePool<T>>::put(candidates);
            }
            <CandidateInfo<T>>::insert(&collator, state);
            Self::deposit_event(Event::CandidateScheduledExit {
                exit_allowed_era: now,
                candidate: collator,
                scheduled_exit: when,
            });
            Ok(().into())
        }

        #[pallet::weight(
			<T as Config>::WeightInfo::execute_leave_candidates(*candidate_nomination_count)
		)]
        /// Execute leave candidates request
        pub fn execute_leave_candidates(
            origin: OriginFor<T>,
            candidate: T::AccountId,
            candidate_nomination_count: u32,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?;
            let state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
            ensure!(
                state.nomination_count <= candidate_nomination_count,
                Error::<T>::TooLowCandidateNominationCountToLeaveCandidates
            );
            state.can_leave::<T>()?;
            let return_stake = |bond: Bond<T::AccountId, BalanceOf<T>>| -> DispatchResult {
                // remove nomination from nominator state
                let mut nominator = NominatorState::<T>::get(&bond.owner).expect(
                    "Collator state and nominator state are consistent.
						Collator state has a record of this nomination. Therefore,
						Nominator state also has a record. qed.",
                );

                if let Some(remaining) = nominator.rm_nomination::<T>(&candidate) {
                    Self::nomination_remove_request_with_state(
                        &candidate,
                        &bond.owner,
                        &mut nominator,
                    );

                    if remaining.is_zero() {
                        // we do not remove the scheduled nomination requests from other collators
                        // since it is assumed that they were removed incrementally before only the
                        // last nomination was left.
                        <NominatorState<T>>::remove(&bond.owner);
                        T::Currency::remove_lock(NOMINATOR_LOCK_ID, &bond.owner);
                    } else {
                        <NominatorState<T>>::insert(&bond.owner, nominator);
                    }
                } else {
                    // TODO: review. we assume here that this nominator has no remaining staked
                    // balance, so we ensure the lock is cleared
                    T::Currency::remove_lock(NOMINATOR_LOCK_ID, &bond.owner);
                }
                Ok(())
            };
            // total backing stake is at least the candidate self bond
            let mut total_backing = state.bond;
            // return all top nominations
            let top_nominations =
                <TopNominations<T>>::take(&candidate).expect("CandidateInfo existence checked");
            for bond in top_nominations.nominations {
                return_stake(bond)?;
            }
            total_backing = total_backing.saturating_add(top_nominations.total);
            // return all bottom nominations
            let bottom_nominations =
                <BottomNominations<T>>::take(&candidate).expect("CandidateInfo existence checked");
            for bond in bottom_nominations.nominations {
                return_stake(bond)?;
            }
            total_backing = total_backing.saturating_add(bottom_nominations.total);
            // return stake to collator
            T::Currency::remove_lock(COLLATOR_LOCK_ID, &candidate);
            <CandidateInfo<T>>::remove(&candidate);
            <NominationScheduledRequests<T>>::remove(&candidate);
            <TopNominations<T>>::remove(&candidate);
            <BottomNominations<T>>::remove(&candidate);
            let new_total_staked = <Total<T>>::get().saturating_sub(total_backing);
            <Total<T>>::put(new_total_staked);
            Self::deposit_event(Event::CandidateLeft {
                ex_candidate: candidate,
                unlocked_amount: total_backing,
                new_total_amt_locked: new_total_staked,
            });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::cancel_leave_candidates(*candidate_count))]
        /// Cancel open request to leave candidates
        /// - only callable by collator account
        /// - result upon successful call is the candidate is active in the candidate pool
        pub fn cancel_leave_candidates(
            origin: OriginFor<T>,
            candidate_count: u32,
        ) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            ensure!(state.is_leaving(), Error::<T>::CandidateNotLeaving);
            state.go_online();
            let mut candidates = <CandidatePool<T>>::get();
            ensure!(
                candidates.0.len() as u32 <= candidate_count,
                Error::<T>::TooLowCandidateCountWeightHintCancelLeaveCandidates
            );
            ensure!(
                candidates.insert(Bond { owner: collator.clone(), amount: state.total_counted }),
                Error::<T>::AlreadyActive
            );
            <CandidatePool<T>>::put(candidates);
            <CandidateInfo<T>>::insert(&collator, state);
            Self::deposit_event(Event::CancelledCandidateExit { candidate: collator });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::go_offline())]
        /// Temporarily leave the set of collator candidates without unbonding
        pub fn go_offline(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            ensure!(state.is_active(), Error::<T>::AlreadyOffline);
            state.go_offline();
            let mut candidates = <CandidatePool<T>>::get();
            if candidates.remove(&Bond::from_owner(collator.clone())) {
                <CandidatePool<T>>::put(candidates);
            }
            <CandidateInfo<T>>::insert(&collator, state);
            Self::deposit_event(Event::CandidateWentOffline { candidate: collator });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::go_online())]
        /// Rejoin the set of collator candidates if previously had called `go_offline`
        pub fn go_online(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            ensure!(!state.is_active(), Error::<T>::AlreadyActive);
            ensure!(!state.is_leaving(), Error::<T>::CannotGoOnlineIfLeaving);
            state.go_online();
            let mut candidates = <CandidatePool<T>>::get();
            ensure!(
                candidates.insert(Bond { owner: collator.clone(), amount: state.total_counted }),
                Error::<T>::AlreadyActive
            );
            <CandidatePool<T>>::put(candidates);
            <CandidateInfo<T>>::insert(&collator, state);
            Self::deposit_event(Event::CandidateBackOnline { candidate: collator });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::candidate_bond_more())]
        /// Increase collator candidate self bond by `more`
        pub fn candidate_bond_more(
            origin: OriginFor<T>,
            more: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            state.bond_more::<T>(collator.clone(), more)?;
            let (is_active, total_counted) = (state.is_active(), state.total_counted);
            <CandidateInfo<T>>::insert(&collator, state);
            if is_active {
                Self::update_active(collator, total_counted);
            }
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::schedule_candidate_bond_less())]
        /// Request by collator candidate to decrease self bond by `less`
        pub fn schedule_candidate_bond_less(
            origin: OriginFor<T>,
            less: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            let when = state.schedule_bond_less::<T>(less)?;
            <CandidateInfo<T>>::insert(&collator, state);
            Self::deposit_event(Event::CandidateBondLessRequested {
                candidate: collator,
                amount_to_decrease: less,
                execute_era: when,
            });
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::execute_candidate_bond_less())]
        /// Execute pending request to adjust the collator candidate self bond
        pub fn execute_candidate_bond_less(
            origin: OriginFor<T>,
            candidate: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?; // we may want to reward this if caller != candidate
            let mut state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
            state.execute_bond_less::<T>(candidate.clone())?;
            <CandidateInfo<T>>::insert(&candidate, state);
            Ok(().into())
        }
        #[pallet::weight(<T as Config>::WeightInfo::cancel_candidate_bond_less())]
        /// Cancel pending request to adjust the collator candidate self bond
        pub fn cancel_candidate_bond_less(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let collator = ensure_signed(origin)?;
            let mut state = <CandidateInfo<T>>::get(&collator).ok_or(Error::<T>::CandidateDNE)?;
            state.cancel_bond_less::<T>(collator.clone())?;
            <CandidateInfo<T>>::insert(&collator, state);
            Ok(().into())
        }
        #[pallet::weight(
			<T as Config>::WeightInfo::nominate(
				*candidate_nomination_count,
				*nomination_count
			)
		)]
        /// If caller is not a nominator and not a collator, then join the set of nominators
        /// If caller is a nominator, then makes nomination to change their nomination state
        pub fn nominate(
            origin: OriginFor<T>,
            candidate: T::AccountId,
            amount: BalanceOf<T>,
            candidate_nomination_count: u32,
            nomination_count: u32,
        ) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            // check that caller can reserve the amount before any changes to storage
            ensure!(
                Self::get_nominator_stakable_free_balance(&nominator) >= amount,
                Error::<T>::InsufficientBalance
            );
            let mut nominator_state = if let Some(mut state) = <NominatorState<T>>::get(&nominator)
            {
                // nomination after first
                ensure!(amount >= T::MinNomination::get(), Error::<T>::NominationBelowMin);
                ensure!(
                    nomination_count >= state.nominations.0.len() as u32,
                    Error::<T>::TooLowNominationCountToNominate
                );
                ensure!(
                    (state.nominations.0.len() as u32) < T::MaxNominationsPerNominator::get(),
                    Error::<T>::ExceedMaxNominationsPerNominator
                );
                ensure!(
                    state.add_nomination(Bond { owner: candidate.clone(), amount }),
                    Error::<T>::AlreadyNominatedCandidate
                );
                state
            } else {
                // first nomination
                ensure!(amount >= T::MinNominatorStk::get(), Error::<T>::NominatorBondBelowMin);
                ensure!(!Self::is_candidate(&nominator), Error::<T>::CandidateExists);
                Nominator::new(nominator.clone(), candidate.clone(), amount)
            };
            let mut state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
            ensure!(
                candidate_nomination_count >= state.nomination_count,
                Error::<T>::TooLowCandidateNominationCountToNominate
            );
            let (nominator_position, less_total_staked) =
                state.add_nomination::<T>(&candidate, Bond { owner: nominator.clone(), amount })?;
            // TODO: causes redundant free_balance check
            nominator_state.adjust_bond_lock::<T>(BondAdjust::Increase(amount))?;
            // only is_some if kicked the lowest bottom as a consequence of this new nomination
            let net_total_increase = if let Some(less) = less_total_staked {
                amount.saturating_sub(less)
            } else {
                amount
            };
            let new_total_locked = <Total<T>>::get().saturating_add(net_total_increase);
            <Total<T>>::put(new_total_locked);
            <CandidateInfo<T>>::insert(&candidate, state);
            <NominatorState<T>>::insert(&nominator, nominator_state);
            Self::deposit_event(Event::Nomination {
                nominator,
                locked_amount: amount,
                candidate,
                nominator_position,
            });
            Ok(().into())
        }

        /// DEPRECATED use batch util with schedule_revoke_nomination for all nominations
        /// Request to leave the set of nominators. If successful, the caller is scheduled to be
        /// allowed to exit via a [NominationAction::Revoke] towards all existing nominations.
        /// Success forbids future nomination requests until the request is invoked or cancelled.
        #[pallet::weight(<T as Config>::WeightInfo::schedule_leave_nominators())]
        pub fn schedule_leave_nominators(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            Self::nominator_schedule_revoke_all(nominator)
        }

        /// DEPRECATED use batch util with execute_nomination_request for all nominations
        /// Execute the right to exit the set of nominators and revoke all ongoing nominations.
        #[pallet::weight(<T as Config>::WeightInfo::execute_leave_nominators(*nomination_count))]
        pub fn execute_leave_nominators(
            origin: OriginFor<T>,
            nominator: T::AccountId,
            nomination_count: u32,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?;
            Self::nominator_execute_scheduled_revoke_all(nominator, nomination_count)
        }

        /// DEPRECATED use batch util with cancel_nomination_request for all nominations
        /// Cancel a pending request to exit the set of nominators. Success clears the pending exit
        /// request (thereby resetting the delay upon another `leave_nominators` call).
        #[pallet::weight(<T as Config>::WeightInfo::cancel_leave_nominators())]
        pub fn cancel_leave_nominators(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            Self::nominator_cancel_scheduled_revoke_all(nominator)
        }

        #[pallet::weight(<T as Config>::WeightInfo::schedule_revoke_nomination())]
        /// Request to revoke an existing nomination. If successful, the nomination is scheduled
        /// to be allowed to be revoked via the `execute_nomination_request` extrinsic.
        pub fn schedule_revoke_nomination(
            origin: OriginFor<T>,
            collator: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            Self::nomination_schedule_revoke(collator, nominator)
        }

        #[pallet::weight(<T as Config>::WeightInfo::nominator_bond_more())]
        /// Bond more for nominators wrt a specific collator candidate.
        pub fn nominator_bond_more(
            origin: OriginFor<T>,
            candidate: T::AccountId,
            more: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            ensure!(
                !Self::nomination_request_revoke_exists(&candidate, &nominator),
                Error::<T>::PendingNominationRevoke
            );
            let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
            state.increase_nomination::<T>(candidate.clone(), more)?;
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::schedule_nominator_bond_less())]
        /// Request bond less for nominators wrt a specific collator candidate.
        pub fn schedule_nominator_bond_less(
            origin: OriginFor<T>,
            candidate: T::AccountId,
            less: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            Self::nomination_schedule_bond_decrease(candidate, nominator, less)
        }

        #[pallet::weight(<T as Config>::WeightInfo::execute_nominator_bond_less())]
        /// Execute pending request to change an existing nomination
        pub fn execute_nomination_request(
            origin: OriginFor<T>,
            nominator: T::AccountId,
            candidate: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?; // we may want to reward caller if caller != nominator
            Self::nomination_execute_scheduled_request(candidate, nominator)
        }

        #[pallet::weight(<T as Config>::WeightInfo::cancel_nominator_bond_less())]
        /// Cancel request to change an existing nomination.
        pub fn cancel_nomination_request(
            origin: OriginFor<T>,
            candidate: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let nominator = ensure_signed(origin)?;
            Self::nomination_cancel_request(candidate, nominator)
        }

        /// Hotfix to remove existing empty entries for candidates that have left.
        #[pallet::weight(
			T::DbWeight::get().reads_writes(2 * candidates.len() as u64, candidates.len() as u64)
		)]
        pub fn hotfix_remove_nomination_requests_exited_candidates(
            origin: OriginFor<T>,
            candidates: Vec<T::AccountId>,
        ) -> DispatchResult {
            ensure_signed(origin)?;
            ensure!(candidates.len() < 100, <Error<T>>::InsufficientBalance);
            for candidate in &candidates {
                ensure!(
                    <CandidateInfo<T>>::get(&candidate).is_none(),
                    <Error<T>>::CandidateNotLeaving
                );
                ensure!(
                    <NominationScheduledRequests<T>>::get(&candidate).is_empty(),
                    <Error<T>>::CandidateNotLeaving
                );
            }

            for candidate in candidates {
                <NominationScheduledRequests<T>>::remove(candidate);
            }

            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn is_nominator(acc: &T::AccountId) -> bool {
            <NominatorState<T>>::get(acc).is_some()
        }
        pub fn is_candidate(acc: &T::AccountId) -> bool {
            <CandidateInfo<T>>::get(acc).is_some()
        }
        pub fn is_selected_candidate(acc: &T::AccountId) -> bool {
            <SelectedCandidates<T>>::get().binary_search(acc).is_ok()
        }
        /// Returns an account's free balance which is not locked in nomination staking
        pub fn get_nominator_stakable_free_balance(acc: &T::AccountId) -> BalanceOf<T> {
            let mut balance = T::Currency::free_balance(acc);
            if let Some(state) = <NominatorState<T>>::get(acc) {
                balance = balance.saturating_sub(state.total());
            }
            balance
        }
        /// Returns an account's free balance which is not locked in collator staking
        pub fn get_collator_stakable_free_balance(acc: &T::AccountId) -> BalanceOf<T> {
            let mut balance = T::Currency::free_balance(acc);
            if let Some(info) = <CandidateInfo<T>>::get(acc) {
                balance = balance.saturating_sub(info.bond);
            }
            balance
        }
        /// Caller must ensure candidate is active before calling
        pub(crate) fn update_active(candidate: T::AccountId, total: BalanceOf<T>) {
            let mut candidates = <CandidatePool<T>>::get();
            candidates.remove(&Bond::from_owner(candidate.clone()));
            candidates.insert(Bond { owner: candidate, amount: total });
            <CandidatePool<T>>::put(candidates);
        }

        /// Compute total reward for era based on the amount in the reward pot
        fn compute_total_reward_to_pay() -> BalanceOf<T> {
            let total_unpaid_reward_amount = Self::reward_pot();
            let mut payout = total_unpaid_reward_amount.checked_sub(&Self::locked_era_payout()).or_else(|| {
				log::error!("� Error calculating era payout. Not enough funds in total_unpaid_reward_amount.");

				//This is a bit strange but since we are dealing with money, log it.
				Self::deposit_event(Event::NotEnoughFundsForEraPayment {reward_pot_balance: total_unpaid_reward_amount});
				Some(BalanceOf::<T>::zero())
			}).expect("We have a default value");

            <LockedEraPayout<T>>::mutate(|lp| {
                *lp = lp
                    .checked_add(&payout)
                    .or_else(|| {
                        log::error!("💔 Error - locked_era_payout overflow. Reducing era payout");
                        // In the unlikely event where the value will overflow the LockedEraPayout,
                        // return the difference to avoid errors
                        payout =
                            BalanceOf::<T>::max_value().saturating_sub(Self::locked_era_payout());
                        Some(BalanceOf::<T>::max_value())
                    })
                    .expect("We have a default value");
            });

            return payout
        }

        /// Remove nomination from candidate state
        /// Amount input should be retrieved from nominator and it informs the storage lookups
        pub(crate) fn nominator_leaves_candidate(
            candidate: T::AccountId,
            nominator: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let mut state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
            state.rm_nomination_if_exists::<T>(&candidate, nominator.clone(), amount)?;
            let new_total_locked = <Total<T>>::get().saturating_sub(amount);
            <Total<T>>::put(new_total_locked);
            let new_total = state.total_counted;
            <CandidateInfo<T>>::insert(&candidate, state);
            Self::deposit_event(Event::NominatorLeftCandidate {
                nominator,
                candidate,
                unstaked_amount: amount,
                total_candidate_staked: new_total,
            });
            Ok(())
        }
        fn prepare_staking_payouts(now: EraIndex) {
            // payout is now - delay eras ago => now - delay > 0 else return early
            let delay = T::RewardPaymentDelay::get();
            if now <= delay {
                return
            }
            let era_to_payout = now.saturating_sub(delay);
            let total_points = <Points<T>>::get(era_to_payout);
            if total_points.is_zero() {
                return
            }
            // Remove stake because it has been processed.
            <Staked<T>>::take(era_to_payout);

            let total_reward_to_pay = Self::compute_total_reward_to_pay();

            let payout = DelayedPayout {
                era_issuance: total_reward_to_pay,
                total_staking_reward: total_reward_to_pay, /* TODO: Remove one of the duplicated
                                                            * fields */
            };

            <DelayedPayouts<T>>::insert(era_to_payout, payout);
        }

        /// Wrapper around pay_one_collator_reward which handles the following logic:
        /// * whether or not a payout needs to be made
        /// * cleaning up when payouts are done
        /// * returns the weight consumed by pay_one_collator_reward if applicable
        fn handle_delayed_payouts(now: EraIndex) -> Weight {
            let delay = T::RewardPaymentDelay::get();

            // don't underflow uint
            if now < delay {
                return 0u64.into()
            }

            let paid_for_era = now.saturating_sub(delay);

            if let Some(payout_info) = <DelayedPayouts<T>>::get(paid_for_era) {
                let result = Self::pay_one_collator_reward(paid_for_era, payout_info);
                if result.0.is_none() {
                    // result.0 indicates whether or not a payout was made
                    // clean up storage items that we no longer need
                    <DelayedPayouts<T>>::remove(paid_for_era);
                    <Points<T>>::remove(paid_for_era);
                }
                result.1 // weight consumed by pay_one_collator_reward
            } else {
                0u64.into()
            }
        }

        /// Payout a single collator from the given era.
        ///
        /// Returns an optional tuple of (Collator's AccountId, total paid)
        /// or None if there were no more payouts to be made for the era.
        pub(crate) fn pay_one_collator_reward(
            paid_for_era: EraIndex,
            payout_info: DelayedPayout<BalanceOf<T>>,
        ) -> (Option<(T::AccountId, BalanceOf<T>)>, Weight) {
            // TODO: it would probably be optimal to roll Points into the DelayedPayouts storage
            // item so that we do fewer reads each block
            let total_points = <Points<T>>::get(paid_for_era);
            if total_points.is_zero() {
                // TODO: this case is obnoxious... it's a value query, so it could mean one of two
                // different logic errors:
                // 1. we removed it before we should have
                // 2. we called pay_one_collator_reward when we were actually done with deferred
                //    payouts
                log::warn!("pay_one_collator_reward called with no <Points<T>> for the era!");
                return (None, 0u64.into())
            }

            let reward_pot_account_id = Self::compute_reward_pot_account_id();
            let pay_reward = |amount: BalanceOf<T>, to: T::AccountId| {
                let result = T::Currency::transfer(
                    &reward_pot_account_id,
                    &to,
                    amount,
                    ExistenceRequirement::KeepAlive,
                );
                if let Ok(_) = result {
                    Self::deposit_event(Event::Rewarded { account: to.clone(), rewards: amount });

                    // Update storage with the amount we paid
                    <LockedEraPayout<T>>::mutate(|p| {
                        *p = p.saturating_sub(amount.into());
                    });
                } else {
                    log::error!("💔 Error paying staking reward: {:?}", result);
                    Self::deposit_event(Event::ErrorPayingStakingReward {
                        payee: to.clone(),
                        rewards: amount,
                    });
                }
            };

            if let Some((collator, pts)) = <AwardedPts<T>>::iter_prefix(paid_for_era).drain().next()
            {
                let mut extra_weight = 0;
                let pct_due = Perbill::from_rational(pts, total_points);
                let total_reward_for_collator = pct_due * payout_info.total_staking_reward;

                // Take the snapshot of block author and nominations
                let state = <AtStake<T>>::take(paid_for_era, &collator);
                let num_nominators = state.nominations.len();

                // pay collator's due portion first
                let collator_pct = Perbill::from_rational(state.bond, state.total);
                let collator_reward = collator_pct * total_reward_for_collator;
                pay_reward(collator_reward, collator.clone());

                // TODO: do we need this?
                extra_weight += T::OnCollatorPayout::on_collator_payout(
                    paid_for_era,
                    collator.clone(),
                    collator_reward,
                );

                // pay nominators due portion, if there are any
                for Bond { owner, amount } in state.nominations {
                    let percent = Perbill::from_rational(amount, state.total);
                    let nominator_reward = percent * total_reward_for_collator;
                    if !nominator_reward.is_zero() {
                        pay_reward(nominator_reward, owner.clone());
                    }
                }

                (
                    Some((collator, total_reward_for_collator)),
                    T::WeightInfo::pay_one_collator_reward(num_nominators as u32) + extra_weight,
                )
            } else {
                // Note that we don't clean up storage here; it is cleaned up in
                // handle_delayed_payouts()
                (None, 0u64.into())
            }
        }

        /// Compute the top `TotalSelected` candidates in the CandidatePool and return
        /// a vec of their AccountIds (in the order of selection)
        pub fn compute_top_candidates() -> Vec<T::AccountId> {
            let mut candidates = <CandidatePool<T>>::get().0;
            // order candidates by stake (least to greatest so requires `rev()`)
            candidates.sort_by(|a, b| a.amount.cmp(&b.amount));
            let top_n = <TotalSelected<T>>::get() as usize;
            // choose the top TotalSelected qualified candidates, ordered by stake
            let mut collators = candidates
                .into_iter()
                .rev()
                .take(top_n)
                .filter(|x| x.amount >= T::MinCollatorStk::get())
                .map(|x| x.owner)
                .collect::<Vec<T::AccountId>>();
            collators.sort();
            collators
        }
        /// Best as in most cumulatively supported in terms of stake
        /// Returns [collator_count, nomination_count, total staked]
        fn select_top_candidates(now: EraIndex) -> (u32, u32, BalanceOf<T>) {
            let (mut collator_count, mut nomination_count, mut total) =
                (0u32, 0u32, BalanceOf::<T>::zero());
            // choose the top TotalSelected qualified candidates, ordered by stake
            let collators = Self::compute_top_candidates();
            if collators.is_empty() {
                // SELECTION FAILED TO SELECT >=1 COLLATOR => select collators from previous era
                let last_era = now.saturating_sub(1u32);
                let mut total_per_candidate: BTreeMap<T::AccountId, BalanceOf<T>> = BTreeMap::new();
                // set this era AtStake to last era AtStake
                for (account, snapshot) in <AtStake<T>>::iter_prefix(last_era) {
                    collator_count = collator_count.saturating_add(1u32);
                    nomination_count =
                        nomination_count.saturating_add(snapshot.nominations.len() as u32);
                    total = total.saturating_add(snapshot.total);
                    total_per_candidate.insert(account.clone(), snapshot.total);
                    <AtStake<T>>::insert(now, account, snapshot);
                }
                // `SelectedCandidates` remains unchanged from last era
                // emit CollatorChosen event for tools that use this event
                for candidate in <SelectedCandidates<T>>::get() {
                    let snapshot_total = total_per_candidate
                        .get(&candidate)
                        .expect("all selected candidates have snapshots");
                    Self::deposit_event(Event::CollatorChosen {
                        era: now,
                        collator_account: candidate,
                        total_exposed_amount: *snapshot_total,
                    })
                }
                return (collator_count, nomination_count, total)
            }

            // snapshot exposure for era for weighting reward distribution
            for account in collators.iter() {
                let state = <CandidateInfo<T>>::get(account)
                    .expect("all members of CandidateQ must be candidates");

                collator_count = collator_count.saturating_add(1u32);
                nomination_count = nomination_count.saturating_add(state.nomination_count);
                total = total.saturating_add(state.total_counted);
                let CountedNominations { uncounted_stake, rewardable_nominations } =
                    Self::get_rewardable_nominators(&account);
                let total_counted = state.total_counted.saturating_sub(uncounted_stake);

                let snapshot = CollatorSnapshot {
                    bond: state.bond,
                    nominations: rewardable_nominations,
                    total: total_counted,
                };
                <AtStake<T>>::insert(now, account, snapshot);
                Self::deposit_event(Event::CollatorChosen {
                    era: now,
                    collator_account: account.clone(),
                    total_exposed_amount: state.total_counted,
                });
            }
            // insert canonical collator set
            <SelectedCandidates<T>>::put(collators);
            (collator_count, nomination_count, total)
        }

        /// Apply the nominator intent for revoke and decrease in order to build the
        /// effective list of nominators with their intended bond amount.
        ///
        /// This will:
        /// - if [NominationChange::Revoke] is outstanding, set the bond amount to 0.
        /// - if [NominationChange::Decrease] is outstanding, subtract the bond by specified amount.
        /// - else, do nothing
        ///
        /// The intended bond amounts will be used while calculating rewards.
        fn get_rewardable_nominators(collator: &T::AccountId) -> CountedNominations<T> {
            let requests = <NominationScheduledRequests<T>>::get(collator)
                .into_iter()
                .map(|x| (x.nominator, x.action))
                .collect::<BTreeMap<_, _>>();
            let mut uncounted_stake = BalanceOf::<T>::zero();
            let rewardable_nominations = <TopNominations<T>>::get(collator)
                .expect("all members of CandidateQ must be candidates")
                .nominations
                .into_iter()
                .map(|mut bond| {
                    bond.amount = match requests.get(&bond.owner) {
                        None => bond.amount,
                        Some(NominationAction::Revoke(_)) => {
                            log::warn!(
                                "reward for nominator '{:?}' set to zero due to pending \
								revoke request",
                                bond.owner
                            );
                            uncounted_stake = uncounted_stake.saturating_add(bond.amount);
                            BalanceOf::<T>::zero()
                        },
                        Some(NominationAction::Decrease(amount)) => {
                            log::warn!(
                                "reward for nominator '{:?}' reduced by set amount due to pending \
								decrease request",
                                bond.owner
                            );
                            uncounted_stake = uncounted_stake.saturating_add(*amount);
                            bond.amount.saturating_sub(*amount)
                        },
                    };

                    bond
                })
                .collect();
            CountedNominations { uncounted_stake, rewardable_nominations }
        }

        /// The account ID of the staking reward_pot.
        /// This actually does computation. If you need to keep using it, then make sure you cache
        /// the value and only call this once.
        pub fn compute_reward_pot_account_id() -> T::AccountId {
            T::RewardPotId::get().into_account_truncating()
        }

        /// The total amount of funds stored in this pallet
        pub fn reward_pot() -> BalanceOf<T> {
            // Must never be less than 0 but better be safe.
            T::Currency::free_balance(&Self::compute_reward_pot_account_id())
                .saturating_sub(T::Currency::minimum_balance())
        }
    }

    /// Keep track of number of authored blocks per authority, uncles are counted as well since
    /// they're a valid proof of being online.
    impl<T: Config + pallet_authorship::Config>
        pallet_authorship::EventHandler<T::AccountId, T::BlockNumber> for Pallet<T>
    {
        /// Add reward points to block authors:
        /// * 20 points to the block producer for producing a block in the chain
        fn note_author(author: T::AccountId) {
            let now = <Era<T>>::get().current;
            let score_plus_20 = <AwardedPts<T>>::get(now, &author).saturating_add(20);
            <AwardedPts<T>>::insert(now, author, score_plus_20);
            <Points<T>>::mutate(now, |x| *x = x.saturating_add(20));

            frame_system::Pallet::<T>::register_extra_weight_unchecked(
                T::WeightInfo::note_author(),
                DispatchClass::Mandatory,
            );
        }

        fn note_uncle(_author: T::AccountId, _age: T::BlockNumber) {
            //TODO: can we ignore this?
        }
    }
}
