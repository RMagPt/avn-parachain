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

#![cfg(feature = "runtime-benchmarks")]

//! Benchmarking
use crate::{
    AwardedPts, BalanceOf, Call, CandidateBondLessRequest, Config, Era, NominationAction, Pallet,
    Points, ScheduledRequest,
};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, vec};
use frame_support::traits::{Currency, Get, OnFinalize, OnInitialize, ReservableCurrency};
use frame_system::RawOrigin;
use sp_runtime::{Perbill, Percent};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// Minimum collator candidate stake
fn min_candidate_stk<T: Config>() -> BalanceOf<T> {
    <<T as Config>::MinCollatorStk as Get<BalanceOf<T>>>::get()
}

/// Minimum nominator stake
fn min_nominator_stk<T: Config>() -> BalanceOf<T> {
    <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get()
}

/// Create a funded user.
/// Extra + min_candidate_stk is total minted funds
/// Returns tuple (id, balance)
fn create_funded_user<T: Config>(
    string: &'static str,
    n: u32,
    extra: BalanceOf<T>,
) -> (T::AccountId, BalanceOf<T>) {
    const SEED: u32 = 0;
    let user = account(string, n, SEED);
    let min_candidate_stk = min_candidate_stk::<T>();
    let total = min_candidate_stk + extra;
    T::Currency::make_free_balance_be(&user, total);
    T::Currency::issue(total);
    (user, total)
}

/// Create a funded nominator.
fn create_funded_nominator<T: Config>(
    string: &'static str,
    n: u32,
    extra: BalanceOf<T>,
    collator: T::AccountId,
    min_bond: bool,
    collator_nominator_count: u32,
) -> Result<T::AccountId, &'static str> {
    let (user, total) = create_funded_user::<T>(string, n, extra);
    let bond = if min_bond { min_nominator_stk::<T>() } else { total };
    Pallet::<T>::nominate(
        RawOrigin::Signed(user.clone()).into(),
        collator,
        bond,
        collator_nominator_count,
        0u32, // first nomination for all calls
    )?;
    Ok(user)
}

/// Create a funded collator.
fn create_funded_collator<T: Config>(
    string: &'static str,
    n: u32,
    extra: BalanceOf<T>,
    min_bond: bool,
    candidate_count: u32,
) -> Result<T::AccountId, &'static str> {
    let (user, total) = create_funded_user::<T>(string, n, extra);
    let bond = if min_bond { min_candidate_stk::<T>() } else { total };
    Pallet::<T>::join_candidates(RawOrigin::Signed(user.clone()).into(), bond, candidate_count)?;
    Ok(user)
}

// Simulate staking on finalize by manually setting points
fn parachain_staking_on_finalize<T: Config>(author: T::AccountId) {
    let now = <Era<T>>::get().current;
    let score_plus_20 = <AwardedPts<T>>::get(now, &author).saturating_add(20);
    <AwardedPts<T>>::insert(now, author, score_plus_20);
    <Points<T>>::mutate(now, |x| *x = x.saturating_add(20));
}

/// Run to end block and author
fn roll_to_and_author<T: Config>(era_delay: u32, author: T::AccountId) {
    let total_eras = era_delay + 1u32;
    let era_length: T::BlockNumber = Pallet::<T>::era().length.into();
    let mut now = <frame_system::Pallet<T>>::block_number() + 1u32.into();
    let end = Pallet::<T>::era().first + (era_length * total_eras.into());
    while now < end {
        parachain_staking_on_finalize::<T>(author.clone());
        <frame_system::Pallet<T>>::on_finalize(<frame_system::Pallet<T>>::block_number());
        <frame_system::Pallet<T>>::set_block_number(
            <frame_system::Pallet<T>>::block_number() + 1u32.into(),
        );
        <frame_system::Pallet<T>>::on_initialize(<frame_system::Pallet<T>>::block_number());
        Pallet::<T>::on_initialize(<frame_system::Pallet<T>>::block_number());
        now += 1u32.into();
    }
}

const USER_SEED: u32 = 999666;

benchmarks! {
    // ROOT DISPATCHABLES

    set_total_selected {
        Pallet::<T>::set_blocks_per_era(RawOrigin::Root.into(), 100u32)?;
    }: _(RawOrigin::Root, 100u32)
    verify {
        assert_eq!(Pallet::<T>::total_selected(), 100u32);
    }

    set_blocks_per_era {}: _(RawOrigin::Root, 1200u32)
    verify {
        assert_eq!(Pallet::<T>::era().length, 1200u32);
    }

    // USER DISPATCHABLES

    join_candidates {
        let x in 3..1_000;
        // Worst Case Complexity is insertion into an ordered list so \exists full list before call
        let mut candidate_count = 1u32;
        for i in 2..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                0u32.into(),
                true,
                candidate_count
            )?;
            candidate_count += 1u32;
        }
        let (caller, min_candidate_stk) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
    }: _(RawOrigin::Signed(caller.clone()), min_candidate_stk, candidate_count)
    verify {
        assert!(Pallet::<T>::is_candidate(&caller));
    }

    // This call schedules the collator's exit and removes them from the candidate pool
    // -> it retains the self-bond and nominator bonds
    schedule_leave_candidates {
        let x in 3..1_000;
        // Worst Case Complexity is removal from an ordered list so \exists full list before call
        let mut candidate_count = 1u32;
        for i in 2..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                0u32.into(),
                true,
                candidate_count
            )?;
            candidate_count += 1u32;
        }
        let caller: T::AccountId = create_funded_collator::<T>(
            "caller",
            USER_SEED,
            0u32.into(),
            true,
            candidate_count,
        )?;
        candidate_count += 1u32;
    }: _(RawOrigin::Signed(caller.clone()), candidate_count)
    verify {
        assert!(Pallet::<T>::candidate_info(&caller).unwrap().is_leaving());
    }

    execute_leave_candidates {
        // x is total number of nominations for the candidate
        let x in 2..(<<T as Config>::MaxTopNominationsPerCandidate as Get<u32>>::get()
        + <<T as Config>::MaxBottomNominationsPerCandidate as Get<u32>>::get());
        let candidate: T::AccountId = create_funded_collator::<T>(
            "unique_caller",
            USER_SEED - 100,
            0u32.into(),
            true,
            1u32,
        )?;
        // 2nd nomination required for all nominators to ensure NominatorState updated not removed
        let second_candidate: T::AccountId = create_funded_collator::<T>(
            "unique__caller",
            USER_SEED - 99,
            0u32.into(),
            true,
            2u32,
        )?;
        let mut nominators: Vec<T::AccountId> = Vec::new();
        let mut col_del_count = 0u32;
        for i in 1..x {
            let seed = USER_SEED + i;
            let nominator = create_funded_nominator::<T>(
                "nominator",
                seed,
                min_nominator_stk::<T>(),
                candidate.clone(),
                true,
                col_del_count,
            )?;
            Pallet::<T>::nominate(
                RawOrigin::Signed(nominator.clone()).into(),
                second_candidate.clone(),
                min_nominator_stk::<T>(),
                col_del_count,
                1u32,
            )?;
            Pallet::<T>::schedule_revoke_nomination(
                RawOrigin::Signed(nominator.clone()).into(),
                candidate.clone()
            )?;
            nominators.push(nominator);
            col_del_count += 1u32;
        }
        Pallet::<T>::schedule_leave_candidates(
            RawOrigin::Signed(candidate.clone()).into(),
            3u32
        )?;
        roll_to_and_author::<T>(2, candidate.clone());
    }: _(RawOrigin::Signed(candidate.clone()), candidate.clone(), col_del_count)
    verify {
        assert!(Pallet::<T>::candidate_info(&candidate).is_none());
        assert!(Pallet::<T>::candidate_info(&second_candidate).is_some());
        for nominator in nominators {
            assert!(Pallet::<T>::is_nominator(&nominator));
        }
    }

    cancel_leave_candidates {
        let x in 3..1_000;
        // Worst Case Complexity is removal from an ordered list so \exists full list before call
        let mut candidate_count = 1u32;
        for i in 2..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                0u32.into(),
                true,
                candidate_count
            )?;
            candidate_count += 1u32;
        }
        let caller: T::AccountId = create_funded_collator::<T>(
            "caller",
            USER_SEED,
            0u32.into(),
            true,
            candidate_count,
        )?;
        candidate_count += 1u32;
        Pallet::<T>::schedule_leave_candidates(
            RawOrigin::Signed(caller.clone()).into(),
            candidate_count
        )?;
        candidate_count -= 1u32;
    }: _(RawOrigin::Signed(caller.clone()), candidate_count)
    verify {
        assert!(Pallet::<T>::candidate_info(&caller).unwrap().is_active());
    }

    go_offline {
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(!Pallet::<T>::candidate_info(&caller).unwrap().is_active());
    }

    go_online {
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        Pallet::<T>::go_offline(RawOrigin::Signed(caller.clone()).into())?;
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(Pallet::<T>::candidate_info(&caller).unwrap().is_active());
    }

    candidate_bond_more {
        let more = min_candidate_stk::<T>();
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            more,
            true,
            1u32,
        )?;
    }: _(RawOrigin::Signed(caller.clone()), more)
    verify {
        let expected_bond = more * 2u32.into();
        assert_eq!(T::Currency::reserved_balance(&caller), expected_bond);
    }

    schedule_candidate_bond_less {
        let min_candidate_stk = min_candidate_stk::<T>();
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            min_candidate_stk,
            false,
            1u32,
        )?;
    }: _(RawOrigin::Signed(caller.clone()), min_candidate_stk)
    verify {
        let state = Pallet::<T>::candidate_info(&caller).expect("request bonded less so exists");
        assert_eq!(
            state.request,
            Some(CandidateBondLessRequest {
                amount: min_candidate_stk,
                when_executable: 3,
            })
        );
    }

    execute_candidate_bond_less {
        let min_candidate_stk = min_candidate_stk::<T>();
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            min_candidate_stk,
            false,
            1u32,
        )?;
        Pallet::<T>::schedule_candidate_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
            min_candidate_stk
        )?;
        roll_to_and_author::<T>(2, caller.clone());
    }: {
        Pallet::<T>::execute_candidate_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
            caller.clone()
        )?;
    } verify {
        assert_eq!(T::Currency::reserved_balance(&caller), min_candidate_stk);
    }

    cancel_candidate_bond_less {
        let min_candidate_stk = min_candidate_stk::<T>();
        let caller: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            min_candidate_stk,
            false,
            1u32,
        )?;
        Pallet::<T>::schedule_candidate_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
            min_candidate_stk
        )?;
    }: {
        Pallet::<T>::cancel_candidate_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
        )?;
    } verify {
        assert!(
            Pallet::<T>::candidate_info(&caller).unwrap().request.is_none()
        );
    }

    nominate {
        let x in 3..<<T as Config>::MaxNominationsPerNominator as Get<u32>>::get();
        let y in 2..<<T as Config>::MaxTopNominationsPerCandidate as Get<u32>>::get();
        // Worst Case is full of nominations before calling `nominate`
        let mut collators: Vec<T::AccountId> = Vec::new();
        // Initialize MaxNominationsPerNominator collator candidates
        for i in 2..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                0u32.into(),
                true,
                collators.len() as u32 + 1u32,
            )?;
            collators.push(collator.clone());
        }
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        let extra = if (bond * (collators.len() as u32 + 1u32).into()) > min_candidate_stk::<T>() {
            (bond * (collators.len() as u32 + 1u32).into()) - min_candidate_stk::<T>()
        } else {
            0u32.into()
        };
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, extra.into());
        // Nomination count
        let mut del_del_count = 0u32;
        // Nominate MaxNominationsPerNominators collator candidates
        for col in collators.clone() {
            Pallet::<T>::nominate(
                RawOrigin::Signed(caller.clone()).into(), col, bond, 0u32, del_del_count
            )?;
            del_del_count += 1u32;
        }
        // Last collator to be nominated
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            collators.len() as u32 + 1u32,
        )?;
        // Worst Case Complexity is insertion into an almost full collator
        let mut col_del_count = 0u32;
        for i in 1..y {
            let seed = USER_SEED + i;
            let _ = create_funded_nominator::<T>(
                "nominator",
                seed,
                0u32.into(),
                collator.clone(),
                true,
                col_del_count,
            )?;
            col_del_count += 1u32;
        }
    }: _(RawOrigin::Signed(caller.clone()), collator, bond, col_del_count, del_del_count)
    verify {
        assert!(Pallet::<T>::is_nominator(&caller));
    }

    schedule_leave_nominators {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(
            Pallet::<T>::nomination_scheduled_requests(&collator)
                .iter()
                .any(|r| r.nominator == caller && matches!(r.action, NominationAction::Revoke(_)))
        );
    }

    execute_leave_nominators {
        let x in 2..<<T as Config>::MaxNominationsPerNominator as Get<u32>>::get();
        // Worst Case is full of nominations before execute exit
        let mut collators: Vec<T::AccountId> = Vec::new();
        // Initialize MaxNominationsPerNominator collator candidates
        for i in 1..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                0u32.into(),
                true,
                collators.len() as u32 + 1u32
            )?;
            collators.push(collator.clone());
        }
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        let need = bond * (collators.len() as u32).into();
        let default_minted = min_candidate_stk::<T>();
        let need: BalanceOf<T> = if need > default_minted {
            need - default_minted
        } else {
            0u32.into()
        };
        // Fund the nominator
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, need);
        // Nomination count
        let mut nomination_count = 0u32;
        let author = collators[0].clone();
        // Nominate MaxNominationsPerNominators collator candidates
        for col in collators {
            Pallet::<T>::nominate(
                RawOrigin::Signed(caller.clone()).into(),
                col,
                bond,
                0u32,
                nomination_count
            )?;
            nomination_count += 1u32;
        }
        Pallet::<T>::schedule_leave_nominators(RawOrigin::Signed(caller.clone()).into())?;
        roll_to_and_author::<T>(2, author);
    }: _(RawOrigin::Signed(caller.clone()), caller.clone(), nomination_count)
    verify {
        assert!(Pallet::<T>::nominator_state(&caller).is_none());
    }

    cancel_leave_nominators {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
        Pallet::<T>::schedule_leave_nominators(RawOrigin::Signed(caller.clone()).into())?;
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(Pallet::<T>::nominator_state(&caller).unwrap().is_active());
    }

    schedule_revoke_nomination {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
    }: _(RawOrigin::Signed(caller.clone()), collator.clone())
    verify {
        assert_eq!(
            Pallet::<T>::nomination_scheduled_requests(&collator),
            vec![ScheduledRequest {
                nominator: caller,
                when_executable: 3,
                action: NominationAction::Revoke(bond),
            }],
        );
    }

    nominator_bond_more {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
    }: _(RawOrigin::Signed(caller.clone()), collator.clone(), bond)
    verify {
        let expected_bond = bond * 2u32.into();
        assert_eq!(T::Currency::reserved_balance(&caller), expected_bond);
    }

    schedule_nominator_bond_less {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, total) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            total,
            0u32,
            0u32
        )?;
        let bond_less = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
    }: _(RawOrigin::Signed(caller.clone()), collator.clone(), bond_less)
    verify {
        let state = Pallet::<T>::nominator_state(&caller)
            .expect("just request bonded less so exists");
        assert_eq!(
            Pallet::<T>::nomination_scheduled_requests(&collator),
            vec![ScheduledRequest {
                nominator: caller,
                when_executable: 3,
                action: NominationAction::Decrease(bond_less),
            }],
        );
    }

    execute_revoke_nomination {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
        Pallet::<T>::schedule_revoke_nomination(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone()
        )?;
        roll_to_and_author::<T>(2, collator.clone());
    }: {
        Pallet::<T>::execute_nomination_request(
            RawOrigin::Signed(caller.clone()).into(),
            caller.clone(),
            collator.clone()
        )?;
    } verify {
        assert!(
            !Pallet::<T>::is_nominator(&caller)
        );
    }

    execute_nominator_bond_less {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, total) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            total,
            0u32,
            0u32
        )?;
        let bond_less = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::schedule_nominator_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone(),
            bond_less
        )?;
        roll_to_and_author::<T>(2, collator.clone());
    }: {
        Pallet::<T>::execute_nomination_request(
            RawOrigin::Signed(caller.clone()).into(),
            caller.clone(),
            collator.clone()
        )?;
    } verify {
        let expected = total - bond_less;
        assert_eq!(T::Currency::reserved_balance(&caller), expected);
    }

    cancel_revoke_nomination {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, _) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let bond = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            bond,
            0u32,
            0u32
        )?;
        Pallet::<T>::schedule_revoke_nomination(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone()
        )?;
    }: {
        Pallet::<T>::cancel_nomination_request(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone()
        )?;
    } verify {
        assert!(
            !Pallet::<T>::nomination_scheduled_requests(&collator)
            .iter()
            .any(|x| &x.nominator == &caller)
        );
    }

    cancel_nominator_bond_less {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let (caller, total) = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        Pallet::<T>::nominate(RawOrigin::Signed(
            caller.clone()).into(),
            collator.clone(),
            total,
            0u32,
            0u32
        )?;
        let bond_less = <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get();
        Pallet::<T>::schedule_nominator_bond_less(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone(),
            bond_less
        )?;
        roll_to_and_author::<T>(2, collator.clone());
    }: {
        Pallet::<T>::cancel_nomination_request(
            RawOrigin::Signed(caller.clone()).into(),
            collator.clone()
        )?;
    } verify {
        assert!(
            !Pallet::<T>::nomination_scheduled_requests(&collator)
                .iter()
                .any(|x| &x.nominator == &caller)
        );
    }

    // ON_INITIALIZE

    era_transition_on_initialize {
        // TOTAL SELECTED COLLATORS PER ERA
        let x in 8..100;
        // NOMINATIONS
        let y in 0..(<<T as Config>::MaxTopNominationsPerCandidate as Get<u32>>::get() * 100);
        let max_nominators_per_collator =
            <<T as Config>::MaxTopNominationsPerCandidate as Get<u32>>::get();
        let max_nominations = x * max_nominators_per_collator;
        // y should depend on x but cannot directly, we overwrite y here if necessary to bound it
        let total_nominations: u32 = if max_nominations < y { max_nominations } else { y };
        // INITIALIZE RUNTIME STATE
        // To set total selected to 40, must first increase era length to at least 40
        // to avoid hitting EraLengthMustBeAtLeastTotalSelectedCollators
        Pallet::<T>::set_blocks_per_era(RawOrigin::Root.into(), 100u32)?;
        Pallet::<T>::set_total_selected(RawOrigin::Root.into(), 100u32)?;
        // INITIALIZE COLLATOR STATE
        let mut collators: Vec<T::AccountId> = Vec::new();
        let mut collator_count = 1u32;
        for i in 0..x {
            let seed = USER_SEED - i;
            let collator = create_funded_collator::<T>(
                "collator",
                seed,
                min_candidate_stk::<T>() * 1_000_000u32.into(),
                true,
                collator_count
            )?;
            collators.push(collator);
            collator_count += 1u32;
        }
        // STORE starting balances for all collators
        let collator_starting_balances: Vec<(
            T::AccountId,
            <<T as Config>::Currency as Currency<T::AccountId>>::Balance
        )> = collators.iter().map(|x| (x.clone(), T::Currency::free_balance(&x))).collect();
        // INITIALIZE NOMINATIONS
        let mut col_del_count: BTreeMap<T::AccountId, u32> = BTreeMap::new();
        collators.iter().for_each(|x| {
            col_del_count.insert(x.clone(), 0u32);
        });
        let mut nominators: Vec<T::AccountId> = Vec::new();
        let mut remaining_nominations = if total_nominations > max_nominators_per_collator {
            for j in 1..(max_nominators_per_collator + 1) {
                let seed = USER_SEED + j;
                let nominator = create_funded_nominator::<T>(
                    "nominator",
                    seed,
                    min_candidate_stk::<T>() * 1_000_000u32.into(),
                    collators[0].clone(),
                    true,
                    nominators.len() as u32,
                )?;
                nominators.push(nominator);
            }
            total_nominations - max_nominators_per_collator
        } else {
            for j in 1..(total_nominations + 1) {
                let seed = USER_SEED + j;
                let nominator = create_funded_nominator::<T>(
                    "nominator",
                    seed,
                    min_candidate_stk::<T>() * 1_000_000u32.into(),
                    collators[0].clone(),
                    true,
                    nominators.len() as u32,
                )?;
                nominators.push(nominator);
            }
            0u32
        };
        col_del_count.insert(collators[0].clone(), nominators.len() as u32);
        // FILL remaining nominations
        if remaining_nominations > 0 {
            for (col, n_count) in col_del_count.iter_mut() {
                if n_count < &mut (nominators.len() as u32) {
                    // assumes nominators.len() <= MaxTopNominationsPerCandidate
                    let mut open_spots = nominators.len() as u32 - *n_count;
                    while open_spots > 0 && remaining_nominations > 0 {
                        let caller = nominators[open_spots as usize - 1usize].clone();
                        if let Ok(_) = Pallet::<T>::nominate(RawOrigin::Signed(
                            caller.clone()).into(),
                            col.clone(),
                            <<T as Config>::MinNominatorStk as Get<BalanceOf<T>>>::get(),
                            *n_count,
                            collators.len() as u32, // overestimate
                        ) {
                            *n_count += 1;
                            remaining_nominations -= 1;
                        }
                        open_spots -= 1;
                    }
                }
                if remaining_nominations == 0 {
                    break;
                }
            }
        }
        // STORE starting balances for all nominators
        let nominator_starting_balances: Vec<(
            T::AccountId,
            <<T as Config>::Currency as Currency<T::AccountId>>::Balance
        )> = nominators.iter().map(|x| (x.clone(), T::Currency::free_balance(&x))).collect();
        // PREPARE RUN_TO_BLOCK LOOP
        let before_running_era_index = Pallet::<T>::era().current;
        let era_length: T::BlockNumber = Pallet::<T>::era().length.into();
        let reward_delay = <<T as Config>::RewardPaymentDelay as Get<u32>>::get() + 2u32;
        let mut now = <frame_system::Pallet<T>>::block_number() + 1u32.into();
        let mut counter = 0usize;
        let end = Pallet::<T>::era().first + (era_length * reward_delay.into());
        // SET collators as authors for blocks from now - end
        while now < end {
            let author = collators[counter % collators.len()].clone();
            parachain_staking_on_finalize::<T>(author);
            <frame_system::Pallet<T>>::on_finalize(<frame_system::Pallet<T>>::block_number());
            <frame_system::Pallet<T>>::set_block_number(
                <frame_system::Pallet<T>>::block_number() + 1u32.into()
            );
            <frame_system::Pallet<T>>::on_initialize(<frame_system::Pallet<T>>::block_number());
            Pallet::<T>::on_initialize(<frame_system::Pallet<T>>::block_number());
            now += 1u32.into();
            counter += 1usize;
        }
        parachain_staking_on_finalize::<T>(collators[counter % collators.len()].clone());
        <frame_system::Pallet<T>>::on_finalize(<frame_system::Pallet<T>>::block_number());
        <frame_system::Pallet<T>>::set_block_number(
            <frame_system::Pallet<T>>::block_number() + 1u32.into()
        );
        <frame_system::Pallet<T>>::on_initialize(<frame_system::Pallet<T>>::block_number());
    }: { Pallet::<T>::on_initialize(<frame_system::Pallet<T>>::block_number()); }
    verify {
        // Collators have been paid
        for (col, initial) in collator_starting_balances {
            assert!(T::Currency::free_balance(&col) > initial);
        }
        // Nominators have been paid
        for (col, initial) in nominator_starting_balances {
            assert!(T::Currency::free_balance(&col) > initial);
        }
        // Era transitions
        assert_eq!(Pallet::<T>::era().current, before_running_era_index + reward_delay);
    }

    pay_one_collator_reward {
        // y controls number of nominations, its maximum per collator is the max top nominations
        let y in 0..<<T as Config>::MaxTopNominationsPerCandidate as Get<u32>>::get();

        // must come after 'let foo in 0..` statements for macro
        use crate::{
            DelayedPayout, DelayedPayouts, AtStake, CollatorSnapshot, Bond, Points,
            AwardedPts,
        };

        let before_running_era_index = Pallet::<T>::era().current;
        let initial_stake_amount = min_candidate_stk::<T>() * 1_000_000u32.into();

        let mut total_staked = 0u32.into();

        // initialize our single collator
        let sole_collator = create_funded_collator::<T>(
            "collator",
            0,
            initial_stake_amount,
            true,
            1u32,
        )?;
        total_staked += initial_stake_amount;

        // generate funded collator accounts
        let mut nominators: Vec<T::AccountId> = Vec::new();
        for i in 0..y {
            let seed = USER_SEED + i;
            let nominator = create_funded_nominator::<T>(
                "nominator",
                seed,
                initial_stake_amount,
                sole_collator.clone(),
                true,
                nominators.len() as u32,
            )?;
            nominators.push(nominator);
            total_staked += initial_stake_amount;
        }

        // rather than roll through eras in order to initialize the storage we want, we set it
        // directly and then call pay_one_collator_reward directly.

        let era_for_payout = 5;
        <DelayedPayouts<T>>::insert(&era_for_payout, DelayedPayout {
            // NOTE: era_issuance is not correct here, but it doesn't seem to cause problems
            era_issuance: 1000u32.into(),
            total_staking_reward: total_staked,
        });

        let mut nominations: Vec<Bond<T::AccountId, BalanceOf<T>>> = Vec::new();
        for nominator in &nominators {
            nominations.push(Bond {
                owner: nominator.clone(),
                amount: 100u32.into(),
            });
        }

        <AtStake<T>>::insert(era_for_payout, &sole_collator, CollatorSnapshot {
            bond: 1_000u32.into(),
            nominations,
            total: 1_000_000u32.into(),
        });

        <Points<T>>::insert(era_for_payout, 100);
        <AwardedPts<T>>::insert(era_for_payout, &sole_collator, 20);

    }: {
        let era_for_payout = 5;
        // TODO: this is an extra read right here (we should whitelist it?)
        let payout_info = Pallet::<T>::delayed_payouts(era_for_payout).expect("payout expected");
        let result = Pallet::<T>::pay_one_collator_reward(era_for_payout, payout_info);
        assert!(result.0.is_some()); // TODO: how to keep this in scope so it can be done in verify block?
    }
    verify {
        // collator should have been paid
        assert!(
            T::Currency::free_balance(&sole_collator) > initial_stake_amount,
            "collator should have been paid in pay_one_collator_reward"
        );
        // nominators should have been paid
        for nominator in &nominators {
            assert!(
                T::Currency::free_balance(&nominator) > initial_stake_amount,
                "nominator should have been paid in pay_one_collator_reward"
            );
        }
    }

    base_on_initialize {
        let collator: T::AccountId = create_funded_collator::<T>(
            "collator",
            USER_SEED,
            0u32.into(),
            true,
            1u32
        )?;
        let start = <frame_system::Pallet<T>>::block_number();
        parachain_staking_on_finalize::<T>(collator.clone());
        <frame_system::Pallet<T>>::on_finalize(start);
        <frame_system::Pallet<T>>::set_block_number(
            start + 1u32.into()
        );
        let end = <frame_system::Pallet<T>>::block_number();
        <frame_system::Pallet<T>>::on_initialize(end);
    }: { Pallet::<T>::on_initialize(end); }
    verify {
        // Era transitions
        assert_eq!(start + 1u32.into(), end);
    }
}

#[cfg(test)]
mod tests {
    use crate::{benchmarks::*, mock::Test};
    use frame_support::assert_ok;
    use sp_io::TestExternalities;

    pub fn new_test_ext() -> TestExternalities {
        let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
        TestExternalities::new(t)
    }

    #[test]
    fn bench_set_staking_expectations() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_set_staking_expectations());
        });
    }

    #[test]
    fn bench_set_parachain_bond_account() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_set_parachain_bond_account());
        });
    }

    #[test]
    fn bench_set_parachain_bond_reserve_percent() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_set_parachain_bond_reserve_percent());
        });
    }

    #[test]
    fn bench_set_total_selected() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_set_total_selected());
        });
    }

    #[test]
    fn bench_set_blocks_per_era() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_set_blocks_per_era());
        });
    }

    #[test]
    fn bench_join_candidates() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_join_candidates());
        });
    }

    #[test]
    fn bench_schedule_leave_candidates() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_schedule_leave_candidates());
        });
    }

    #[test]
    fn bench_execute_leave_candidates() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_execute_leave_candidates());
        });
    }

    #[test]
    fn bench_cancel_leave_candidates() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_cancel_leave_candidates());
        });
    }

    #[test]
    fn bench_go_offline() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_go_offline());
        });
    }

    #[test]
    fn bench_go_online() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_go_online());
        });
    }

    #[test]
    fn bench_candidate_bond_more() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_candidate_bond_more());
        });
    }

    #[test]
    fn bench_schedule_candidate_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_schedule_candidate_bond_less());
        });
    }

    #[test]
    fn bench_execute_candidate_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_execute_candidate_bond_less());
        });
    }

    #[test]
    fn bench_cancel_candidate_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_cancel_candidate_bond_less());
        });
    }

    #[test]
    fn bench_nominate() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_nominate());
        });
    }

    #[test]
    fn bench_schedule_leave_nominators() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_schedule_leave_nominators());
        });
    }

    #[test]
    fn bench_execute_leave_nominators() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_execute_leave_nominators());
        });
    }

    #[test]
    fn bench_cancel_leave_nominators() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_cancel_leave_nominators());
        });
    }

    #[test]
    fn bench_schedule_revoke_nomination() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_schedule_revoke_nomination());
        });
    }

    #[test]
    fn bench_nominator_bond_more() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_nominator_bond_more());
        });
    }

    #[test]
    fn bench_schedule_nominator_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_schedule_nominator_bond_less());
        });
    }

    #[test]
    fn bench_execute_revoke_nomination() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_execute_revoke_nomination());
        });
    }

    #[test]
    fn bench_execute_nominator_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_execute_nominator_bond_less());
        });
    }

    #[test]
    fn bench_cancel_revoke_nomination() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_cancel_revoke_nomination());
        });
    }

    #[test]
    fn bench_cancel_nominator_bond_less() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_cancel_nominator_bond_less());
        });
    }

    #[test]
    fn bench_era_transition_on_initialize() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_era_transition_on_initialize());
        });
    }

    #[test]
    fn bench_base_on_initialize() {
        new_test_ext().execute_with(|| {
            assert_ok!(Pallet::<Test>::test_benchmark_base_on_initialize());
        });
    }
}

impl_benchmark_test_suite!(Pallet, crate::benchmarks::tests::new_test_ext(), crate::mock::Test);
