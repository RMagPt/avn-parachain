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

//! # Staking Pallet Unit Tests
//! The unit tests are organized by the call they test. The order matches the order
//! of the calls in the `lib.rs`.
//! 1. Root
//! 2. Monetary Governance
//! 3. Public (Collator, Nominator)
//! 4. Miscellaneous Property-Based Tests
use crate::{
    assert_eq_events, assert_eq_last_events, assert_event_emitted, assert_last_event,
    assert_tail_eq,
    mock::{
        roll_one_block, roll_to, roll_to_era_begin, roll_to_era_end, set_author, set_reward_pot,
        Balances, Event as MetaEvent, ExtBuilder, Origin, ParachainStaking, Test,
    },
    nomination_requests::{CancelledScheduledRequest, NominationAction, ScheduledRequest},
    AtStake, Bond, CollatorStatus, Error, Event, NominationScheduledRequests, NominatorAdded,
    NominatorState, NominatorStatus, NOMINATOR_LOCK_ID,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{traits::Zero, DispatchError, ModuleError};

// ~~ ROOT ~~

#[test]
fn invalid_root_origin_fails() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::set_total_selected(Origin::signed(45), 6u32),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_noop!(
            ParachainStaking::set_blocks_per_era(Origin::signed(45), 3u32),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// SET TOTAL SELECTED

#[test]
fn set_total_selected_event_emits_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        // before we can bump total_selected we must bump the blocks per era
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 6u32));
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 6u32));
        assert_last_event!(MetaEvent::ParachainStaking(Event::TotalSelectedSet {
            old: 5u32,
            new: 6u32
        }));
    });
}

#[test]
fn set_total_selected_fails_if_above_blocks_per_era() {
    ExtBuilder::default().build().execute_with(|| {
        assert_eq!(ParachainStaking::era().length, 5); // test relies on this
        assert_noop!(
            ParachainStaking::set_total_selected(Origin::root(), 6u32),
            Error::<Test>::EraLengthMustBeAtLeastTotalSelectedCollators,
        );
    });
}

#[test]
fn set_total_selected_passes_if_equal_to_blocks_per_era() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 10u32));
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 10u32));
    });
}

#[test]
fn set_total_selected_passes_if_below_blocks_per_era() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 10u32));
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 9u32));
    });
}

#[test]
fn set_blocks_per_era_fails_if_below_total_selected() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 20u32));
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 15u32));
        assert_noop!(
            ParachainStaking::set_blocks_per_era(Origin::root(), 14u32),
            Error::<Test>::EraLengthMustBeAtLeastTotalSelectedCollators,
        );
    });
}

#[test]
fn set_blocks_per_era_passes_if_equal_to_total_selected() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 10u32));
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 9u32));
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 9u32));
    });
}

#[test]
fn set_blocks_per_era_passes_if_above_total_selected() {
    ExtBuilder::default().build().execute_with(|| {
        assert_eq!(ParachainStaking::era().length, 5); // test relies on this
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 6u32));
    });
}

#[test]
fn set_total_selected_storage_updates_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        // era length must be >= total_selected, so update that first
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 10u32));

        assert_eq!(ParachainStaking::total_selected(), 5u32);
        assert_ok!(ParachainStaking::set_total_selected(Origin::root(), 6u32));
        assert_eq!(ParachainStaking::total_selected(), 6u32);
    });
}

#[test]
fn cannot_set_total_selected_to_current_total_selected() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::set_total_selected(Origin::root(), 5u32),
            Error::<Test>::NoWritingSameValue
        );
    });
}

#[test]
fn cannot_set_total_selected_below_module_min() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::set_total_selected(Origin::root(), 4u32),
            Error::<Test>::CannotSetBelowMin
        );
    });
}

// SET BLOCKS PER ERA

#[test]
fn set_blocks_per_era_event_emits_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 6u32));
        assert_last_event!(MetaEvent::ParachainStaking(Event::BlocksPerEraSet {
            current_era: 1,
            first_block: 0,
            old: 5,
            new: 6,
        }));
    });
}

#[test]
fn set_blocks_per_era_storage_updates_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        assert_eq!(ParachainStaking::era().length, 5);
        assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 6u32));
        assert_eq!(ParachainStaking::era().length, 6);
    });
}

#[test]
fn cannot_set_blocks_per_era_below_module_min() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::set_blocks_per_era(Origin::root(), 2u32),
            Error::<Test>::CannotSetBelowMin
        );
    });
}

#[test]
fn cannot_set_blocks_per_era_to_current_blocks_per_era() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::set_blocks_per_era(Origin::root(), 5u32),
            Error::<Test>::NoWritingSameValue
        );
    });
}

#[test]
fn era_immediately_jumps_if_current_duration_exceeds_new_blocks_per_era() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // we can't lower the blocks per era because it must be above the number of collators,
            // and we can't lower the number of collators because it must be above
            // MinSelectedCandidates. so we first raise blocks per era, then lower it.
            assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 10u32));

            roll_to(17);
            assert_last_event!(MetaEvent::ParachainStaking(Event::NewEra {
                starting_block: 10,
                era: 2,
                selected_collators_number: 1,
                total_balance: 20
            }));
            assert_ok!(ParachainStaking::set_blocks_per_era(Origin::root(), 5u32));
            roll_to(18);
            assert_last_event!(MetaEvent::ParachainStaking(Event::NewEra {
                starting_block: 18,
                era: 3,
                selected_collators_number: 1,
                total_balance: 20
            }));
        });
}

// ~~ PUBLIC ~~

// JOIN CANDIDATES

#[test]
fn join_candidates_event_emits_correctly() {
    ExtBuilder::default().with_balances(vec![(1, 10)]).build().execute_with(|| {
        assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 10u128, 0u32));
        assert_last_event!(MetaEvent::ParachainStaking(Event::JoinedCollatorCandidates {
            account: 1,
            amount_locked: 10u128,
            new_total_amt_locked: 10u128,
        }));
    });
}

#[test]
fn join_candidates_reserves_balance() {
    ExtBuilder::default().with_balances(vec![(1, 10)]).build().execute_with(|| {
        assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 10);
        assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 10u128, 0u32));
        assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 0);
    });
}

#[test]
fn join_candidates_increases_total_staked() {
    ExtBuilder::default().with_balances(vec![(1, 10)]).build().execute_with(|| {
        assert_eq!(ParachainStaking::total(), 0);
        assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 10u128, 0u32));
        assert_eq!(ParachainStaking::total(), 10);
    });
}

#[test]
fn join_candidates_creates_candidate_state() {
    ExtBuilder::default().with_balances(vec![(1, 10)]).build().execute_with(|| {
        assert!(ParachainStaking::candidate_info(1).is_none());
        assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 10u128, 0u32));
        let candidate_state = ParachainStaking::candidate_info(1).expect("just joined => exists");
        assert_eq!(candidate_state.bond, 10u128);
    });
}

#[test]
fn join_candidates_adds_to_candidate_pool() {
    ExtBuilder::default().with_balances(vec![(1, 10)]).build().execute_with(|| {
        assert!(ParachainStaking::candidate_pool().0.is_empty());
        assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 10u128, 0u32));
        let candidate_pool = ParachainStaking::candidate_pool();
        assert_eq!(candidate_pool.0[0].owner, 1);
        assert_eq!(candidate_pool.0[0].amount, 10);
    });
}

#[test]
fn cannot_join_candidates_if_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 1000)])
        .with_candidates(vec![(1, 500)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::join_candidates(Origin::signed(1), 11u128, 100u32),
                Error::<Test>::CandidateExists
            );
        });
}

#[test]
fn cannot_join_candidates_if_nominator() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50), (2, 20)])
        .with_candidates(vec![(1, 50)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::join_candidates(Origin::signed(2), 10u128, 1u32),
                Error::<Test>::NominatorExists
            );
        });
}

#[test]
fn cannot_join_candidates_without_min_bond() {
    ExtBuilder::default().with_balances(vec![(1, 1000)]).build().execute_with(|| {
        assert_noop!(
            ParachainStaking::join_candidates(Origin::signed(1), 9u128, 100u32),
            Error::<Test>::CandidateBondBelowMin
        );
    });
}

#[test]
fn cannot_join_candidates_with_more_than_available_balance() {
    ExtBuilder::default().with_balances(vec![(1, 500)]).build().execute_with(|| {
        assert_noop!(
            ParachainStaking::join_candidates(Origin::signed(1), 501u128, 100u32),
            DispatchError::Module(ModuleError {
                index: 2,
                error: [8, 0, 0, 0],
                message: Some("InsufficientBalance")
            })
        );
    });
}

#[test]
fn insufficient_join_candidates_weight_hint_fails() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .build()
        .execute_with(|| {
            for i in 0..5 {
                assert_noop!(
                    ParachainStaking::join_candidates(Origin::signed(6), 20, i),
                    Error::<Test>::TooLowCandidateCountWeightHintJoinCandidates
                );
            }
        });
}

#[test]
fn sufficient_join_candidates_weight_hint_succeeds() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (2, 20),
            (3, 20),
            (4, 20),
            (5, 20),
            (6, 20),
            (7, 20),
            (8, 20),
            (9, 20),
        ])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .build()
        .execute_with(|| {
            let mut count = 5u32;
            for i in 6..10 {
                assert_ok!(ParachainStaking::join_candidates(Origin::signed(i), 20, count));
                count += 1u32;
            }
        });
}

// SCHEDULE LEAVE CANDIDATES

#[test]
fn leave_candidates_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateScheduledExit {
                exit_allowed_era: 1,
                candidate: 1,
                scheduled_exit: 3
            }));
        });
}

#[test]
fn leave_candidates_removes_candidate_from_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::candidate_pool().0.len(), 1);
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert!(ParachainStaking::candidate_pool().0.is_empty());
        });
}

#[test]
fn cannot_leave_candidates_if_not_candidate() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32),
            Error::<Test>::CandidateDNE
        );
    });
}

#[test]
fn cannot_leave_candidates_if_already_leaving_candidates() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_noop!(
                ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32),
                Error::<Test>::CandidateAlreadyLeaving
            );
        });
}

#[test]
fn insufficient_leave_candidates_weight_hint_fails() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .build()
        .execute_with(|| {
            for i in 1..6 {
                assert_noop!(
                    ParachainStaking::schedule_leave_candidates(Origin::signed(i), 4u32),
                    Error::<Test>::TooLowCandidateCountToLeaveCandidates
                );
            }
        });
}

#[test]
fn sufficient_leave_candidates_weight_hint_succeeds() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20)])
        .build()
        .execute_with(|| {
            let mut count = 5u32;
            for i in 1..6 {
                assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(i), count));
                count -= 1u32;
            }
        });
}

// EXECUTE LEAVE CANDIDATES

#[test]
fn execute_leave_candidates_emits_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 0));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateLeft {
                ex_candidate: 1,
                unlocked_amount: 10,
                new_total_amt_locked: 0
            }));
        });
}

#[test]
fn execute_leave_candidates_callable_by_any_signed() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(2), 1, 0));
        });
}

#[test]
fn execute_leave_candidates_requires_correct_weight_hint() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10), (2, 10), (3, 10), (4, 10)])
        .with_candidates(vec![(1, 10)])
        .with_nominations(vec![(2, 1, 10), (3, 1, 10), (4, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            roll_to(10);
            for i in 0..3 {
                assert_noop!(
                    ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, i),
                    Error::<Test>::TooLowCandidateNominationCountToLeaveCandidates
                );
            }
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(2), 1, 3));
        });
}

#[test]
fn execute_leave_candidates_unreserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 0);
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 0));
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 10);
        });
}

#[test]
fn execute_leave_candidates_decreases_total_staked() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 10);
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 0));
            assert_eq!(ParachainStaking::total(), 0);
        });
}

#[test]
fn execute_leave_candidates_removes_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            // candidate state is not immediately removed
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("just left => still exists");
            assert_eq!(candidate_state.bond, 10u128);
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 0));
            assert!(ParachainStaking::candidate_info(1).is_none());
        });
}

#[test]
fn execute_leave_candidates_removes_pending_nomination_requests() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10), (2, 15)])
        .with_candidates(vec![(1, 10)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            let state = ParachainStaking::nomination_scheduled_requests(&1);
            assert_eq!(
                state,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            // candidate state is not immediately removed
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("just left => still exists");
            assert_eq!(candidate_state.bond, 10u128);
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 1));
            assert!(ParachainStaking::candidate_info(1).is_none());
            assert!(
                !ParachainStaking::nomination_scheduled_requests(&1)
                    .iter()
                    .any(|x| x.nominator == 2),
                "nomination request not removed"
            );
            assert!(
                !<NominationScheduledRequests<Test>>::contains_key(&1),
                "the key was not removed from storage"
            );
        });
}

#[test]
fn cannot_execute_leave_candidates_before_delay() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_noop!(
                ParachainStaking::execute_leave_candidates(Origin::signed(3), 1, 0),
                Error::<Test>::CandidateCannotLeaveYet
            );
            roll_to(9);
            assert_noop!(
                ParachainStaking::execute_leave_candidates(Origin::signed(3), 1, 0),
                Error::<Test>::CandidateCannotLeaveYet
            );
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(3), 1, 0));
        });
}

// CANCEL LEAVE CANDIDATES

#[test]
fn cancel_leave_candidates_emits_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_ok!(ParachainStaking::cancel_leave_candidates(Origin::signed(1), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CancelledCandidateExit {
                candidate: 1
            }));
        });
}

#[test]
fn cancel_leave_candidates_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_ok!(ParachainStaking::cancel_leave_candidates(Origin::signed(1), 1));
            let candidate =
                ParachainStaking::candidate_info(&1).expect("just cancelled leave so exists");
            assert!(candidate.is_active());
        });
}

#[test]
fn cancel_leave_candidates_adds_to_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10)])
        .with_candidates(vec![(1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1u32));
            assert_ok!(ParachainStaking::cancel_leave_candidates(Origin::signed(1), 1));
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 10);
        });
}

// GO OFFLINE

#[test]
fn go_offline_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateWentOffline {
                candidate: 1
            }));
        });
}

#[test]
fn go_offline_removes_candidate_from_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::candidate_pool().0.len(), 1);
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            assert!(ParachainStaking::candidate_pool().0.is_empty());
        });
}

#[test]
fn go_offline_updates_candidate_state_to_idle() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            let candidate_state = ParachainStaking::candidate_info(1).expect("is active candidate");
            assert_eq!(candidate_state.status, CollatorStatus::Active);
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("is candidate, just offline");
            assert_eq!(candidate_state.status, CollatorStatus::Idle);
        });
}

#[test]
fn cannot_go_offline_if_not_candidate() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(ParachainStaking::go_offline(Origin::signed(3)), Error::<Test>::CandidateDNE);
    });
}

#[test]
fn cannot_go_offline_if_already_offline() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            assert_noop!(
                ParachainStaking::go_offline(Origin::signed(1)),
                Error::<Test>::AlreadyOffline
            );
        });
}

// GO ONLINE

#[test]
fn go_online_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            assert_ok!(ParachainStaking::go_online(Origin::signed(1)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateBackOnline {
                candidate: 1
            }));
        });
}

#[test]
fn go_online_adds_to_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            assert!(ParachainStaking::candidate_pool().0.is_empty());
            assert_ok!(ParachainStaking::go_online(Origin::signed(1)));
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 20);
        });
}

#[test]
fn go_online_storage_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::go_offline(Origin::signed(1)));
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("offline still exists");
            assert_eq!(candidate_state.status, CollatorStatus::Idle);
            assert_ok!(ParachainStaking::go_online(Origin::signed(1)));
            let candidate_state = ParachainStaking::candidate_info(1).expect("online so exists");
            assert_eq!(candidate_state.status, CollatorStatus::Active);
        });
}

#[test]
fn cannot_go_online_if_not_candidate() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(ParachainStaking::go_online(Origin::signed(3)), Error::<Test>::CandidateDNE);
    });
}

#[test]
fn cannot_go_online_if_already_online() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::go_online(Origin::signed(1)),
                Error::<Test>::AlreadyActive
            );
        });
}

#[test]
fn cannot_go_online_if_leaving() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_noop!(
                ParachainStaking::go_online(Origin::signed(1)),
                Error::<Test>::CannotGoOnlineIfLeaving
            );
        });
}

// CANDIDATE BOND MORE

#[test]
fn candidate_bond_more_emits_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::candidate_bond_more(Origin::signed(1), 30));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateBondedMore {
                candidate: 1,
                amount: 30,
                new_total_bond: 50
            }));
        });
}

#[test]
fn candidate_bond_more_reserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 30);
            assert_ok!(ParachainStaking::candidate_bond_more(Origin::signed(1), 30));
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 0);
        });
}

#[test]
fn candidate_bond_more_increases_total() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            let mut total = ParachainStaking::total();
            assert_ok!(ParachainStaking::candidate_bond_more(Origin::signed(1), 30));
            total += 30;
            assert_eq!(ParachainStaking::total(), total);
        });
}

#[test]
fn candidate_bond_more_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            let candidate_state = ParachainStaking::candidate_info(1).expect("updated => exists");
            assert_eq!(candidate_state.bond, 20);
            assert_ok!(ParachainStaking::candidate_bond_more(Origin::signed(1), 30));
            let candidate_state = ParachainStaking::candidate_info(1).expect("updated => exists");
            assert_eq!(candidate_state.bond, 50);
        });
}

#[test]
fn candidate_bond_more_updates_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 20);
            assert_ok!(ParachainStaking::candidate_bond_more(Origin::signed(1), 30));
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 50);
        });
}

// SCHEDULE CANDIDATE BOND LESS

#[test]
fn schedule_candidate_bond_less_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateBondLessRequested {
                candidate: 1,
                amount_to_decrease: 10,
                execute_era: 3,
            }));
        });
}

#[test]
fn cannot_schedule_candidate_bond_less_if_request_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 5));
            assert_noop!(
                ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 5),
                Error::<Test>::PendingCandidateRequestAlreadyExists
            );
        });
}

#[test]
fn cannot_schedule_candidate_bond_less_if_not_candidate() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::schedule_candidate_bond_less(Origin::signed(6), 50),
            Error::<Test>::CandidateDNE
        );
    });
}

#[test]
fn cannot_schedule_candidate_bond_less_if_new_total_below_min_candidate_stk() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 21),
                Error::<Test>::CandidateBondBelowMin
            );
        });
}

#[test]
fn can_schedule_candidate_bond_less_if_leaving_candidates() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
        });
}

#[test]
fn cannot_schedule_candidate_bond_less_if_exited_candidates() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 0));
            assert_noop!(
                ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10),
                Error::<Test>::CandidateDNE
            );
        });
}

// 2. EXECUTE BOND LESS REQUEST

#[test]
fn execute_candidate_bond_less_emits_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 50)])
        .with_candidates(vec![(1, 50)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 30));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_candidate_bond_less(Origin::signed(1), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateBondedLess {
                candidate: 1,
                amount: 30,
                new_bond: 20
            }));
        });
}

#[test]
fn execute_candidate_bond_less_unreserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 0);
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_candidate_bond_less(Origin::signed(1), 1));
            assert_eq!(ParachainStaking::get_collator_stakable_free_balance(&1), 10);
        });
}

#[test]
fn execute_candidate_bond_less_decreases_total() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            let mut total = ParachainStaking::total();
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_candidate_bond_less(Origin::signed(1), 1));
            total -= 10;
            assert_eq!(ParachainStaking::total(), total);
        });
}

#[test]
fn execute_candidate_bond_less_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            let candidate_state = ParachainStaking::candidate_info(1).expect("updated => exists");
            assert_eq!(candidate_state.bond, 30);
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_candidate_bond_less(Origin::signed(1), 1));
            let candidate_state = ParachainStaking::candidate_info(1).expect("updated => exists");
            assert_eq!(candidate_state.bond, 20);
        });
}

#[test]
fn execute_candidate_bond_less_updates_candidate_pool() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 30);
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_candidate_bond_less(Origin::signed(1), 1));
            assert_eq!(ParachainStaking::candidate_pool().0[0].owner, 1);
            assert_eq!(ParachainStaking::candidate_pool().0[0].amount, 20);
        });
}

// CANCEL CANDIDATE BOND LESS REQUEST

#[test]
fn cancel_candidate_bond_less_emits_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            assert_ok!(ParachainStaking::cancel_candidate_bond_less(Origin::signed(1)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CancelledCandidateBondLess {
                candidate: 1,
                amount: 10,
                execute_era: 3,
            }));
        });
}

#[test]
fn cancel_candidate_bond_less_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            assert_ok!(ParachainStaking::cancel_candidate_bond_less(Origin::signed(1)));
            assert!(ParachainStaking::candidate_info(&1).unwrap().request.is_none());
        });
}

#[test]
fn only_candidate_can_cancel_candidate_bond_less_request() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_candidate_bond_less(Origin::signed(1), 10));
            assert_noop!(
                ParachainStaking::cancel_candidate_bond_less(Origin::signed(2)),
                Error::<Test>::CandidateDNE
            );
        });
}

// NOMINATE

#[test]
fn nominate_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 10, 0, 0));
            assert_last_event!(MetaEvent::ParachainStaking(Event::Nomination {
                nominator: 2,
                locked_amount: 10,
                candidate: 1,
                nominator_position: NominatorAdded::AddedToTop { new_total: 40 },
            }));
        });
}

#[test]
fn nominate_reserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 10);
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 10, 0, 0));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 0);
        });
}

#[test]
fn nominate_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert!(ParachainStaking::nominator_state(2).is_none());
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 10, 0, 0));
            let nominator_state =
                ParachainStaking::nominator_state(2).expect("just nominated => exists");
            assert_eq!(nominator_state.total(), 10);
            assert_eq!(nominator_state.nominations.0[0].owner, 1);
            assert_eq!(nominator_state.nominations.0[0].amount, 10);
        });
}

#[test]
fn nominate_updates_collator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("registered in genesis");
            assert_eq!(candidate_state.total_counted, 30);
            let top_nominations =
                ParachainStaking::top_nominations(1).expect("registered in genesis");
            assert!(top_nominations.nominations.is_empty());
            assert!(top_nominations.total.is_zero());
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 10, 0, 0));
            let candidate_state =
                ParachainStaking::candidate_info(1).expect("just nominated => exists");
            assert_eq!(candidate_state.total_counted, 40);
            let top_nominations =
                ParachainStaking::top_nominations(1).expect("just nominated => exists");
            assert_eq!(top_nominations.nominations[0].owner, 2);
            assert_eq!(top_nominations.nominations[0].amount, 10);
            assert_eq!(top_nominations.total, 10);
        });
}

#[test]
fn can_nominate_immediately_after_other_join_candidates() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::join_candidates(Origin::signed(1), 20, 0));
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 20, 0, 0));
        });
}

#[test]
fn can_nominate_if_revoking() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 30), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 4, 10, 0, 2));
        });
}

#[test]
fn cannot_nominate_if_full_and_new_nomination_less_than_or_equal_lowest_bottom() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (2, 10),
            (3, 10),
            (4, 10),
            (5, 10),
            (6, 10),
            (7, 10),
            (8, 10),
            (9, 10),
            (10, 10),
            (11, 10),
        ])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![
            (2, 1, 10),
            (3, 1, 10),
            (4, 1, 10),
            (5, 1, 10),
            (6, 1, 10),
            (8, 1, 10),
            (9, 1, 10),
            (10, 1, 10),
        ])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::nominate(Origin::signed(11), 1, 10, 8, 0),
                Error::<Test>::CannotNominateLessThanOrEqualToLowestBottomWhenFull
            );
        });
}

#[test]
fn can_nominate_if_full_and_new_nomination_greater_than_lowest_bottom() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (2, 10),
            (3, 10),
            (4, 10),
            (5, 10),
            (6, 10),
            (7, 10),
            (8, 10),
            (9, 10),
            (10, 10),
            (11, 11),
        ])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![
            (2, 1, 10),
            (3, 1, 10),
            (4, 1, 10),
            (5, 1, 10),
            (6, 1, 10),
            (8, 1, 10),
            (9, 1, 10),
            (10, 1, 10),
        ])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::nominate(Origin::signed(11), 1, 11, 8, 0));
            assert_event_emitted!(Event::NominationKicked {
                nominator: 10,
                candidate: 1,
                unstaked_amount: 10
            });
            assert_event_emitted!(Event::NominatorLeft { nominator: 10, unstaked_amount: 10 });
        });
}

#[test]
fn can_still_nominate_if_leaving() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 20), (3, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 3, 10, 0, 1),);
        });
}

#[test]
fn cannot_nominate_if_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 30)])
        .with_candidates(vec![(1, 20), (2, 20)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::nominate(Origin::signed(2), 1, 10, 0, 0),
                Error::<Test>::CandidateExists
            );
        });
}

#[test]
fn cannot_nominate_if_already_nominated() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 30)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 20)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::nominate(Origin::signed(2), 1, 10, 1, 1),
                Error::<Test>::AlreadyNominatedCandidate
            );
        });
}

#[test]
fn cannot_nominate_more_than_max_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 50), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10), (2, 4, 10), (2, 5, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::nominate(Origin::signed(2), 6, 10, 0, 4),
                Error::<Test>::ExceedMaxNominationsPerNominator,
            );
        });
}

#[test]
fn sufficient_nominate_weight_hint_succeeds() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (2, 20),
            (3, 20),
            (4, 20),
            (5, 20),
            (6, 20),
            (7, 20),
            (8, 20),
            (9, 20),
            (10, 20),
        ])
        .with_candidates(vec![(1, 20), (2, 20)])
        .with_nominations(vec![(3, 1, 10), (4, 1, 10), (5, 1, 10), (6, 1, 10)])
        .build()
        .execute_with(|| {
            let mut count = 4u32;
            for i in 7..11 {
                assert_ok!(ParachainStaking::nominate(Origin::signed(i), 1, 10, count, 0u32));
                count += 1u32;
            }
            let mut count = 0u32;
            for i in 3..11 {
                assert_ok!(ParachainStaking::nominate(Origin::signed(i), 2, 10, count, 1u32));
                count += 1u32;
            }
        });
}

#[test]
fn insufficient_nominate_weight_hint_fails() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (2, 20),
            (3, 20),
            (4, 20),
            (5, 20),
            (6, 20),
            (7, 20),
            (8, 20),
            (9, 20),
            (10, 20),
        ])
        .with_candidates(vec![(1, 20), (2, 20)])
        .with_nominations(vec![(3, 1, 10), (4, 1, 10), (5, 1, 10), (6, 1, 10)])
        .build()
        .execute_with(|| {
            let mut count = 3u32;
            for i in 7..11 {
                assert_noop!(
                    ParachainStaking::nominate(Origin::signed(i), 1, 10, count, 0u32),
                    Error::<Test>::TooLowCandidateNominationCountToNominate
                );
            }
            // to set up for next error test
            count = 4u32;
            for i in 7..11 {
                assert_ok!(ParachainStaking::nominate(Origin::signed(i), 1, 10, count, 0u32));
                count += 1u32;
            }
            count = 0u32;
            for i in 3..11 {
                assert_noop!(
                    ParachainStaking::nominate(Origin::signed(i), 2, 10, count, 0u32),
                    Error::<Test>::TooLowNominationCountToNominate
                );
                count += 1u32;
            }
        });
}

// SCHEDULE LEAVE NOMINATORS

#[test]
fn schedule_leave_nominators_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitScheduled {
                era: 1,
                nominator: 2,
                scheduled_exit: 3
            }));
        });
}

#[test]
fn cannot_schedule_leave_nominators_if_already_leaving() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_noop!(
                ParachainStaking::schedule_leave_nominators(Origin::signed(2)),
                Error::<Test>::NominatorAlreadyLeaving
            );
        });
}

#[test]
fn cannot_schedule_leave_nominators_if_not_nominator() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_leave_nominators(Origin::signed(2)),
                Error::<Test>::NominatorDNE
            );
        });
}

// EXECUTE LEAVE NOMINATORS

#[test]
fn execute_leave_nominators_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeft { nominator: 2, unstaked_amount: 10 });
        });
}

#[test]
fn execute_leave_nominators_unreserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 00);
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 10);
            assert_eq!(crate::mock::query_lock_amount(2, NOMINATOR_LOCK_ID), None);
        });
}

#[test]
fn execute_leave_nominators_decreases_total_staked() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::total(), 30);
        });
}

#[test]
fn execute_leave_nominators_removes_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert!(ParachainStaking::nominator_state(2).is_some());
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert!(ParachainStaking::nominator_state(2).is_none());
        });
}

#[test]
fn execute_leave_nominators_removes_pending_nomination_requests() {
    ExtBuilder::default()
        .with_balances(vec![(1, 10), (2, 15)])
        .with_candidates(vec![(1, 10)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            let state = ParachainStaking::nomination_scheduled_requests(&1);
            assert_eq!(
                state,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert!(ParachainStaking::nominator_state(2).is_none());
            assert!(
                !ParachainStaking::nomination_scheduled_requests(&1)
                    .iter()
                    .any(|x| x.nominator == 2),
                "nomination request not removed"
            )
        });
}

#[test]
fn execute_leave_nominators_removes_nominations_from_collator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 100), (2, 20), (3, 20), (4, 20), (5, 20)])
        .with_candidates(vec![(2, 20), (3, 20), (4, 20), (5, 20)])
        .with_nominations(vec![(1, 2, 10), (1, 3, 10), (1, 4, 10), (1, 5, 10)])
        .build()
        .execute_with(|| {
            for i in 2..6 {
                let candidate_state =
                    ParachainStaking::candidate_info(i).expect("initialized in ext builder");
                assert_eq!(candidate_state.total_counted, 30);
                let top_nominations =
                    ParachainStaking::top_nominations(i).expect("initialized in ext builder");
                assert_eq!(top_nominations.nominations[0].owner, 1);
                assert_eq!(top_nominations.nominations[0].amount, 10);
                assert_eq!(top_nominations.total, 10);
            }
            assert_eq!(ParachainStaking::nominator_state(1).unwrap().nominations.0.len(), 4usize);
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(1)));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(1), 1, 10));
            for i in 2..6 {
                let candidate_state =
                    ParachainStaking::candidate_info(i).expect("initialized in ext builder");
                assert_eq!(candidate_state.total_counted, 20);
                let top_nominations =
                    ParachainStaking::top_nominations(i).expect("initialized in ext builder");
                assert!(top_nominations.nominations.is_empty());
            }
        });
}

#[test]
fn cannot_execute_leave_nominators_before_delay() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_noop!(
                ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1),
                Error::<Test>::NominatorCannotLeaveYet
            );
            // can execute after delay
            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
        });
}

#[test]
fn cannot_execute_leave_nominators_if_single_nomination_revoke_manually_cancelled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 3));
            roll_to(10);
            assert_noop!(
                ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 2),
                Error::<Test>::NominatorNotLeaving
            );
            // can execute after manually scheduling revoke, and the era delay after which
            // all revokes can be executed
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3));
            roll_to(20);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 2));
        });
}

#[test]
fn insufficient_execute_leave_nominators_weight_hint_fails() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(3, 1, 10), (4, 1, 10), (5, 1, 10), (6, 1, 10)])
        .build()
        .execute_with(|| {
            for i in 3..7 {
                assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(i)));
            }
            roll_to(10);
            for i in 3..7 {
                assert_noop!(
                    ParachainStaking::execute_leave_nominators(Origin::signed(i), i, 0),
                    Error::<Test>::TooLowNominationCountToLeaveNominators
                );
            }
        });
}

#[test]
fn sufficient_execute_leave_nominators_weight_hint_succeeds() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(3, 1, 10), (4, 1, 10), (5, 1, 10), (6, 1, 10)])
        .build()
        .execute_with(|| {
            for i in 3..7 {
                assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(i)));
            }
            roll_to(10);
            for i in 3..7 {
                assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(i), i, 1));
            }
        });
}

// CANCEL LEAVE NOMINATORS

#[test]
fn cancel_leave_nominators_emits_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_ok!(ParachainStaking::cancel_leave_nominators(Origin::signed(2)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitCancelled {
                nominator: 2
            }));
        });
}

#[test]
fn cancel_leave_nominators_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_ok!(ParachainStaking::cancel_leave_nominators(Origin::signed(2)));
            let nominator =
                ParachainStaking::nominator_state(&2).expect("just cancelled exit so exists");
            assert!(nominator.is_active());
        });
}

#[test]
fn cannot_cancel_leave_nominators_if_single_nomination_revoke_manually_cancelled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 3));
            roll_to(10);
            assert_noop!(
                ParachainStaking::cancel_leave_nominators(Origin::signed(2)),
                Error::<Test>::NominatorNotLeaving
            );
            // can execute after manually scheduling revoke, without waiting for era delay after
            // which all revokes can be executed
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3));
            assert_ok!(ParachainStaking::cancel_leave_nominators(Origin::signed(2)));
        });
}

// SCHEDULE REVOKE NOMINATION

#[test]
fn revoke_nomination_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationRevocationScheduled {
                era: 1,
                nominator: 2,
                candidate: 1,
                scheduled_exit: 3,
            }));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeftCandidate {
                nominator: 2,
                candidate: 1,
                unstaked_amount: 10,
                total_candidate_staked: 30
            });
        });
}

#[test]
fn can_revoke_nomination_if_revoking_another_nomination() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            // this is an exit implicitly because last nomination revoked
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3));
        });
}

#[test]
fn nominator_not_allowed_revoke_if_already_leaving() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_noop!(
                ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3),
                <Error<Test>>::PendingNominationRequestAlreadyExists,
            );
        });
}

#[test]
fn cannot_revoke_nomination_if_not_nominator() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1),
            Error::<Test>::NominatorDNE
        );
    });
}

#[test]
fn cannot_revoke_nomination_that_dne() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3),
                Error::<Test>::NominationDNE
            );
        });
}

#[test]
// See `cannot_execute_revoke_nomination_below_min_nominator_stake` for where the "must be above
// MinNominatorStk" rule is now enforced.
fn can_schedule_revoke_nomination_below_min_nominator_stake() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 8), (3, 20)])
        .with_candidates(vec![(1, 20), (3, 20)])
        .with_nominations(vec![(2, 1, 5), (2, 3, 3)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
        });
}

// NOMINATOR BOND MORE

#[test]
fn nominator_bond_more_reserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 5);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 0);
        });
}

#[test]
fn nominator_bond_more_increases_total_staked() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_eq!(ParachainStaking::total(), 45);
        });
}

#[test]
fn nominator_bond_more_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::nominator_state(2).expect("exists").total(), 10);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_eq!(ParachainStaking::nominator_state(2).expect("exists").total(), 15);
        });
}

#[test]
fn nominator_bond_more_updates_candidate_state_top_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].owner, 2);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].amount, 10);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().total, 10);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].owner, 2);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].amount, 15);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().total, 15);
        });
}

#[test]
fn nominator_bond_more_updates_candidate_state_bottom_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20), (4, 20), (5, 20), (6, 20)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10), (3, 1, 20), (4, 1, 20), (5, 1, 20), (6, 1, 20)])
        .build()
        .execute_with(|| {
            assert_eq!(
                ParachainStaking::bottom_nominations(1).expect("exists").nominations[0].owner,
                2
            );
            assert_eq!(
                ParachainStaking::bottom_nominations(1).expect("exists").nominations[0].amount,
                10
            );
            assert_eq!(ParachainStaking::bottom_nominations(1).unwrap().total, 10);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationIncreased {
                nominator: 2,
                candidate: 1,
                amount: 5,
                in_top: false
            }));
            assert_eq!(
                ParachainStaking::bottom_nominations(1).expect("exists").nominations[0].owner,
                2
            );
            assert_eq!(
                ParachainStaking::bottom_nominations(1).expect("exists").nominations[0].amount,
                15
            );
            assert_eq!(ParachainStaking::bottom_nominations(1).unwrap().total, 15);
        });
}

#[test]
fn nominator_bond_more_increases_total() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
            assert_eq!(ParachainStaking::total(), 45);
        });
}

#[test]
fn can_nominator_bond_more_for_leaving_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
        });
}

#[test]
fn nominator_bond_more_disallowed_when_revoke_scheduled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_noop!(
                ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5),
                <Error<Test>>::PendingNominationRevoke
            );
        });
}

#[test]
fn nominator_bond_more_allowed_when_bond_decrease_scheduled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5,));
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 1, 5));
        });
}

// NOMINATOR BOND LESS

#[test]
fn nominator_bond_less_event_emits_correctly() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationDecreaseScheduled {
                nominator: 2,
                candidate: 1,
                amount_to_decrease: 5,
                execute_era: 3,
            }));
        });
}

#[test]
fn nominator_bond_less_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            let state = ParachainStaking::nomination_scheduled_requests(&1);
            assert_eq!(
                state,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
        });
}

#[test]
fn nominator_not_allowed_bond_less_if_leaving() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 1),
                <Error<Test>>::PendingNominationRequestAlreadyExists,
            );
        });
}

#[test]
fn cannot_nominator_bond_less_if_revoking() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 1),
                Error::<Test>::PendingNominationRequestAlreadyExists
            );
        });
}

#[test]
fn cannot_nominator_bond_less_if_not_nominator() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5),
            Error::<Test>::NominatorDNE
        );
    });
}

#[test]
fn cannot_nominator_bond_less_if_candidate_dne() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 3, 5),
                Error::<Test>::NominationDNE
            );
        });
}

#[test]
fn cannot_nominator_bond_less_if_nomination_dne() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 3, 5),
                Error::<Test>::NominationDNE
            );
        });
}

#[test]
fn cannot_nominator_bond_less_below_min_collator_stk() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 6),
                Error::<Test>::NominatorBondBelowMin
            );
        });
}

#[test]
fn cannot_nominator_bond_less_more_than_total_nomination() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 11),
                Error::<Test>::NominatorBondBelowMin
            );
        });
}

#[test]
fn cannot_nominator_bond_less_below_min_nomination() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 8),
                Error::<Test>::NominationBelowMin
            );
        });
}

// EXECUTE PENDING NOMINATION REQUEST

// 1. REVOKE NOMINATION

#[test]
fn execute_revoke_nomination_emits_exit_event_if_exit_happens() {
    // last nomination is revocation
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeftCandidate {
                nominator: 2,
                candidate: 1,
                unstaked_amount: 10,
                total_candidate_staked: 30
            });
            assert_event_emitted!(Event::NominatorLeft { nominator: 2, unstaked_amount: 10 });
        });
}

#[test]
fn cannot_execute_revoke_nomination_below_min_nominator_stake() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 8), (3, 20)])
        .with_candidates(vec![(1, 20), (3, 20)])
        .with_nominations(vec![(2, 1, 5), (2, 3, 3)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_noop!(
                ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1),
                Error::<Test>::NominatorBondBelowMin
            );
            // but nominator can cancel the request and request to leave instead:
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            roll_to(20);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 2));
        });
}

#[test]
fn revoke_nomination_executes_exit_if_last_nomination() {
    // last nomination is revocation
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeftCandidate {
                nominator: 2,
                candidate: 1,
                unstaked_amount: 10,
                total_candidate_staked: 30
            });
            assert_event_emitted!(Event::NominatorLeft { nominator: 2, unstaked_amount: 10 });
        });
}

#[test]
fn execute_revoke_nomination_emits_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeftCandidate {
                nominator: 2,
                candidate: 1,
                unstaked_amount: 10,
                total_candidate_staked: 30
            });
        });
}

#[test]
fn execute_revoke_nomination_unreserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 0);
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 10);
        });
}

#[test]
fn execute_revoke_nomination_adds_revocation_to_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert!(!ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert!(ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
        });
}

#[test]
fn execute_revoke_nomination_removes_revocation_from_nominator_state_upon_execution() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert!(!ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
        });
}

#[test]
fn execute_revoke_nomination_removes_revocation_from_state_for_single_nomination_leave() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 20), (3, 20)])
        .with_candidates(vec![(1, 30), (3, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert!(
                !ParachainStaking::nomination_scheduled_requests(&1)
                    .iter()
                    .any(|x| x.nominator == 2),
                "nomination was not removed"
            );
        });
}

#[test]
fn execute_revoke_nomination_decreases_total_staked() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::total(), 30);
        });
}

#[test]
fn execute_revoke_nomination_for_last_nomination_removes_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert!(ParachainStaking::nominator_state(2).is_some());
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            // this will be confusing for people
            // if status is leaving, then execute_nomination_request works if last nomination
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert!(ParachainStaking::nominator_state(2).is_none());
        });
}

#[test]
fn execute_revoke_nomination_removes_nomination_from_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::candidate_info(1).expect("exists").nomination_count, 1u32);
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert!(ParachainStaking::candidate_info(1)
                .expect("exists")
                .nomination_count
                .is_zero());
        });
}

#[test]
fn can_execute_revoke_nomination_for_leaving_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            // can execute nomination request for leaving candidate
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
        });
}

#[test]
fn can_execute_leave_candidates_if_revoking_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            roll_to(10);
            // revocation executes during execute leave candidates (callable by anyone)
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(1), 1, 1));
            assert!(!ParachainStaking::is_nominator(&2));
            assert_eq!(Balances::reserved_balance(&2), 0);
            assert_eq!(Balances::free_balance(&2), 10);
        });
}

#[test]
fn nominator_bond_more_after_revoke_nomination_does_not_effect_exit() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 30), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(2), 3, 10));
            roll_to(100);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert!(ParachainStaking::is_nominator(&2));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 10);
        });
}

#[test]
fn nominator_bond_less_after_revoke_nomination_does_not_effect_exit() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 30), (3, 30)])
        .with_candidates(vec![(1, 30), (3, 30)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationRevocationScheduled {
                era: 1,
                nominator: 2,
                candidate: 1,
                scheduled_exit: 3,
            }));
            assert_noop!(
                ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 2),
                Error::<Test>::PendingNominationRequestAlreadyExists
            );
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 3, 2));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 3));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationDecreased {
                nominator: 2,
                candidate: 3,
                amount: 2,
                in_top: true
            }));
            assert!(ParachainStaking::is_nominator(&2));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 22);
        });
}

// 2. EXECUTE BOND LESS

#[test]
fn execute_nominator_bond_less_unreserves_balance() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 0);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&2), 5);
        });
}

#[test]
fn execute_nominator_bond_less_decreases_total_staked() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::total(), 35);
        });
}

#[test]
fn execute_nominator_bond_less_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::nominator_state(2).expect("exists").total(), 10);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::nominator_state(2).expect("exists").total(), 5);
        });
}

#[test]
fn execute_nominator_bond_less_updates_candidate_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].owner, 2);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].amount, 10);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].owner, 2);
            assert_eq!(ParachainStaking::top_nominations(1).unwrap().nominations[0].amount, 5);
        });
}

#[test]
fn execute_nominator_bond_less_decreases_total() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(ParachainStaking::total(), 35);
        });
}

#[test]
fn execute_nominator_bond_less_updates_just_bottom_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 10), (3, 11), (4, 12), (5, 14), (6, 15)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10), (3, 1, 11), (4, 1, 12), (5, 1, 14), (6, 1, 15)])
        .build()
        .execute_with(|| {
            let pre_call_candidate_info =
                ParachainStaking::candidate_info(&1).expect("nominated by all so exists");
            let pre_call_top_nominations =
                ParachainStaking::top_nominations(&1).expect("nominated by all so exists");
            let pre_call_bottom_nominations =
                ParachainStaking::bottom_nominations(&1).expect("nominated by all so exists");
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 2));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            let post_call_candidate_info =
                ParachainStaking::candidate_info(&1).expect("nominated by all so exists");
            let post_call_top_nominations =
                ParachainStaking::top_nominations(&1).expect("nominated by all so exists");
            let post_call_bottom_nominations =
                ParachainStaking::bottom_nominations(&1).expect("nominated by all so exists");
            let mut not_equal = false;
            for Bond { owner, amount } in pre_call_bottom_nominations.nominations {
                for Bond { owner: post_owner, amount: post_amount } in
                    &post_call_bottom_nominations.nominations
                {
                    if &owner == post_owner {
                        if &amount != post_amount {
                            not_equal = true;
                            break
                        }
                    }
                }
            }
            assert!(not_equal);
            let mut equal = true;
            for Bond { owner, amount } in pre_call_top_nominations.nominations {
                for Bond { owner: post_owner, amount: post_amount } in
                    &post_call_top_nominations.nominations
                {
                    if &owner == post_owner {
                        if &amount != post_amount {
                            equal = false;
                            break
                        }
                    }
                }
            }
            assert!(equal);
            assert_eq!(
                pre_call_candidate_info.total_counted,
                post_call_candidate_info.total_counted
            );
        });
}

#[test]
fn execute_nominator_bond_less_does_not_delete_bottom_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 10), (3, 11), (4, 12), (5, 14), (6, 15)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10), (3, 1, 11), (4, 1, 12), (5, 1, 14), (6, 1, 15)])
        .build()
        .execute_with(|| {
            let pre_call_candidate_info =
                ParachainStaking::candidate_info(&1).expect("nominated by all so exists");
            let pre_call_top_nominations =
                ParachainStaking::top_nominations(&1).expect("nominated by all so exists");
            let pre_call_bottom_nominations =
                ParachainStaking::bottom_nominations(&1).expect("nominated by all so exists");
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(6), 1, 4));
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(6), 6, 1));
            let post_call_candidate_info =
                ParachainStaking::candidate_info(&1).expect("nominated by all so exists");
            let post_call_top_nominations =
                ParachainStaking::top_nominations(&1).expect("nominated by all so exists");
            let post_call_bottom_nominations =
                ParachainStaking::bottom_nominations(&1).expect("nominated by all so exists");
            let mut equal = true;
            for Bond { owner, amount } in pre_call_bottom_nominations.nominations {
                for Bond { owner: post_owner, amount: post_amount } in
                    &post_call_bottom_nominations.nominations
                {
                    if &owner == post_owner {
                        if &amount != post_amount {
                            equal = false;
                            break
                        }
                    }
                }
            }
            assert!(equal);
            let mut not_equal = false;
            for Bond { owner, amount } in pre_call_top_nominations.nominations {
                for Bond { owner: post_owner, amount: post_amount } in
                    &post_call_top_nominations.nominations
                {
                    if &owner == post_owner {
                        if &amount != post_amount {
                            not_equal = true;
                            break
                        }
                    }
                }
            }
            assert!(not_equal);
            assert_eq!(
                pre_call_candidate_info.total_counted - 4,
                post_call_candidate_info.total_counted
            );
        });
}

#[test]
fn can_execute_nominator_bond_less_for_leaving_candidate() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(1), 1));
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            roll_to(10);
            // can execute bond more nomination request for leaving candidate
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
        });
}

// CANCEL PENDING NOMINATION REQUEST
// 1. CANCEL REVOKE NOMINATION

#[test]
fn cancel_revoke_nomination_emits_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CancelledNominationRequest {
                nominator: 2,
                collator: 1,
                cancelled_request: CancelledScheduledRequest {
                    when_executable: 3,
                    action: NominationAction::Revoke(10),
                },
            }));
        });
}

#[test]
fn cancel_revoke_nomination_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 10)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            let state = ParachainStaking::nomination_scheduled_requests(&1);
            assert_eq!(
                state,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Revoke(10),
                }],
            );
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                10
            );
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));
            assert!(!ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                0
            );
        });
}

// 2. CANCEL NOMINATOR BOND LESS

#[test]
fn cancel_nominator_bond_less_correct_event() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CancelledNominationRequest {
                nominator: 2,
                collator: 1,
                cancelled_request: CancelledScheduledRequest {
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                },
            }));
        });
}

#[test]
fn cancel_nominator_bond_less_updates_nominator_state() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 15)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 5));
            let state = ParachainStaking::nomination_scheduled_requests(&1);
            assert_eq!(
                state,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                5
            );
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));
            assert!(!ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                0
            );
        });
}

// ~~ PROPERTY-BASED TESTS ~~

#[test]
fn nominator_schedule_revocation_total() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20), (5, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20), (5, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10), (2, 4, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                10
            );
            roll_to(10);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 1));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                0
            );
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 5, 10, 0, 2));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 3));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 4));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                20,
            );
            roll_to(20);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 3));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                10,
            );
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(2), 2, 4));
            assert_eq!(
                ParachainStaking::nominator_state(&2)
                    .map(|x| x.less_total)
                    .expect("nominator state must exist"),
                0
            );
        });
}

#[test]
fn parachain_bond_inflation_reserve_matches_config() {
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
            (11, 1),
        ])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20), (5, 10)])
        .with_nominations(vec![(6, 1, 10), (7, 1, 10), (8, 2, 10), (9, 2, 10), (10, 1, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(Balances::free_balance(&11), 1);
            roll_to(8);
            // chooses top TotalSelectedCandidates (5), in order
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
            ];
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for all blocks this era
            set_author(2, 1, 100);
            // We now payout from a central pot so we need to fund it
            set_reward_pot(50);
            roll_to(16);
            // distribute total issuance to collator 1 and its nominators 6, 7, 19
            let mut new = vec![
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 4, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 4, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 4, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded {
                    /*Explanation of how reward is computed:
                        Total staked = 50
                        Total reward to be paid = 50
                        Collator stake = 20

                        collator gets 40% ([collator stake] * 100 / [total staked]) of the [total reward] = 20 (40 * 50 / 100)
                        Total 20
                    */
                    account: 1,
                    rewards: 20,
                },
                Event::Rewarded {
                    /*Explanation of how reward is computed:
                        Total staked = 50
                        Total reward to be paid = 50
                        Nominator stake = 10

                        nominator gets 20% ([nominator stake] * 100 / [total staked]) of 50 ([total reward]) = 10
                        Total 10
                    */
                    account: 6,
                    rewards: 10,
                },
                Event::Rewarded { account: 7, rewards: 10 },
                Event::Rewarded { account: 10, rewards: 10 },
            ];
            expected.append(&mut new);
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for all blocks this era
            set_author(3, 1, 100);
            set_author(4, 1, 100);
            set_author(5, 1, 100);
            // 1. ensure nominators are paid for 2 eras after they leave
            assert_noop!(
                ParachainStaking::schedule_leave_nominators(Origin::signed(66)),
                Error::<Test>::NominatorDNE
            );
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(6)));
            // fast forward to block in which nominator 6 exit executes. Doing it in 2 steps so we
            // can reset the reward pot
            set_reward_pot(55);
            roll_to(20);

            set_reward_pot(56);
            roll_to(25);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(6), 6, 10));
            set_reward_pot(58);
            roll_to(30);
            let mut new2 = vec![
                Event::NominatorExitScheduled { era: 4, nominator: 6, scheduled_exit: 6 },
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 5, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 5, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 5, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 22 },
                Event::Rewarded { account: 6, rewards: 11 },
                Event::Rewarded { account: 7, rewards: 11 },
                Event::Rewarded { account: 10, rewards: 11 },
                Event::CollatorChosen { era: 6, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 6, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 6, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 6, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 6, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 25,
                    era: 6,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 22 },
                Event::Rewarded { account: 6, rewards: 11 },
                Event::Rewarded { account: 7, rewards: 11 },
                Event::Rewarded { account: 10, rewards: 11 },
                Event::NominatorLeftCandidate {
                    nominator: 6,
                    candidate: 1,
                    unstaked_amount: 10,
                    total_candidate_staked: 40,
                },
                Event::NominatorLeft { nominator: 6, unstaked_amount: 10 },
                Event::CollatorChosen { era: 7, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 7, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 7, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 7, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 7, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 30,
                    era: 7,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 29 },
                Event::Rewarded { account: 7, rewards: 14 },
                Event::Rewarded { account: 10, rewards: 14 },
            ];
            expected.append(&mut new2);
            assert_eq_events!(expected.clone());
            // 6 won't be paid for this era because they left already
            set_author(6, 1, 100);
            set_reward_pot(61);
            roll_to(35);
            // keep paying 6
            let mut new3 = vec![
                Event::CollatorChosen { era: 8, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 8, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 8, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 8, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 8, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 35,
                    era: 8,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 30 },
                Event::Rewarded { account: 7, rewards: 15 },
                Event::Rewarded { account: 10, rewards: 15 },
            ];
            expected.append(&mut new3);
            assert_eq_events!(expected.clone());
            set_author(7, 1, 100);
            set_reward_pot(64);
            roll_to(40);
            // no more paying 6
            let mut new4 = vec![
                Event::CollatorChosen { era: 9, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 9, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 9, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 9, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 9, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 40,
                    era: 9,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 32 },
                Event::Rewarded { account: 7, rewards: 16 },
                Event::Rewarded { account: 10, rewards: 16 },
            ];
            expected.append(&mut new4);
            assert_eq_events!(expected.clone());
            set_author(8, 1, 100);
            assert_ok!(ParachainStaking::nominate(Origin::signed(8), 1, 10, 10, 10));
            set_reward_pot(67);
            roll_to(45);
            // new nomination is not rewarded yet
            let mut new5 = vec![
                Event::Nomination {
                    nominator: 8,
                    locked_amount: 10,
                    candidate: 1,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 50 },
                },
                Event::CollatorChosen { era: 10, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 10, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 10, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 10, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 10, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 45,
                    era: 10,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 33 },
                Event::Rewarded { account: 7, rewards: 17 },
                Event::Rewarded { account: 10, rewards: 17 },
            ];
            expected.append(&mut new5);
            assert_eq_events!(expected.clone());
            set_author(9, 1, 100);
            set_author(10, 1, 100);
            set_reward_pot(70);
            roll_to(50);
            // new nomination is still not rewarded yet
            let mut new6 = vec![
                Event::CollatorChosen { era: 11, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 11, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 11, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 11, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 11, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 50,
                    era: 11,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 35 },
                Event::Rewarded { account: 7, rewards: 17 },
                Event::Rewarded { account: 10, rewards: 17 },
            ];
            expected.append(&mut new6);
            assert_eq_events!(expected.clone());
            set_reward_pot(75);
            roll_to(55);
            // new nomination is rewarded, 2 eras after joining (`RewardPaymentDelay` is 2)
            let mut new7 = vec![
                Event::CollatorChosen { era: 12, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 12, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 12, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 12, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 12, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 55,
                    era: 12,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 30 },
                Event::Rewarded { account: 7, rewards: 15 },
                Event::Rewarded { account: 10, rewards: 15 },
                Event::Rewarded { account: 8, rewards: 15 },
            ];
            expected.append(&mut new7);
            assert_eq_events!(expected);
        });
}

#[test]
fn rewards_matches_config() {
    ExtBuilder::default()
        .with_balances(vec![(1, 100), (2, 100), (3, 100), (4, 100), (5, 100), (6, 100)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10), (3, 1, 10)])
        .build()
        .execute_with(|| {
            roll_to(8);
            // chooses top TotalSelectedCandidates (5), in order
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 40 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 1,
                    total_balance: 40,
                },
            ];
            assert_eq_events!(expected.clone());
            assert_ok!(ParachainStaking::join_candidates(Origin::signed(4), 20u128, 100u32));
            assert_last_event!(MetaEvent::ParachainStaking(Event::JoinedCollatorCandidates {
                account: 4,
                amount_locked: 20u128,
                new_total_amt_locked: 60u128,
            }));
            roll_to(9);
            assert_ok!(ParachainStaking::nominate(Origin::signed(5), 4, 10, 10, 10));
            assert_ok!(ParachainStaking::nominate(Origin::signed(6), 4, 10, 10, 10));
            roll_to(11);
            let mut new = vec![
                Event::JoinedCollatorCandidates {
                    account: 4,
                    amount_locked: 20,
                    new_total_amt_locked: 60,
                },
                Event::Nomination {
                    nominator: 5,
                    locked_amount: 10,
                    candidate: 4,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 30 },
                },
                Event::Nomination {
                    nominator: 6,
                    locked_amount: 10,
                    candidate: 4,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 40 },
                },
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 40 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 2,
                    total_balance: 80,
                },
            ];
            expected.append(&mut new);
            assert_eq_events!(expected.clone());
            // only reward author with id 4
            set_author(3, 4, 100);
            set_reward_pot(30);
            roll_to(21);
            // 20% of 10 is commission + due_portion (0) = 2 + 4 = 6
            // all nominator payouts are 10-2 = 8 * stake_pct
            let mut new2 = vec![
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 40 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 2,
                    total_balance: 80,
                },
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 40 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 2,
                    total_balance: 80,
                },
                Event::Rewarded { account: 4, rewards: 15 },
                Event::Rewarded { account: 5, rewards: 7 },
                Event::Rewarded { account: 6, rewards: 7 },
            ];
            expected.append(&mut new2);
            assert_eq_events!(expected);
        });
}

#[test]
fn collator_exit_executes_after_delay() {
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
            roll_to(11);
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(2), 2));
            let info = ParachainStaking::candidate_info(&2).unwrap();
            assert_eq!(info.status, CollatorStatus::Leaving(5));
            roll_to(21);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(2), 2, 2));
            // we must exclude leaving collators from rewards while
            // holding them retroactively accountable for previous faults
            // (within the last T::SlashingWindow blocks)
            let expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 700 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 400 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 2,
                    total_balance: 1100,
                },
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 700 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 400 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 2,
                    total_balance: 1100,
                },
                Event::CandidateScheduledExit {
                    exit_allowed_era: 3,
                    candidate: 2,
                    scheduled_exit: 5,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 700 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 1,
                    total_balance: 700,
                },
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 700 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 1,
                    total_balance: 700,
                },
                Event::CandidateLeft {
                    ex_candidate: 2,
                    unlocked_amount: 400,
                    new_total_amt_locked: 700,
                },
            ];
            assert_eq_events!(expected);
        });
}

#[test]
fn collator_selection_chooses_top_candidates() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 1000),
            (2, 1000),
            (3, 1000),
            (4, 1000),
            (5, 1000),
            (6, 1000),
            (7, 33),
            (8, 33),
            (9, 33),
        ])
        .with_candidates(vec![(1, 100), (2, 90), (3, 80), (4, 70), (5, 60), (6, 50)])
        .build()
        .execute_with(|| {
            roll_to(8);
            // should choose top TotalSelectedCandidates (5), in order
            let expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
            ];
            assert_eq_events!(expected.clone());
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(6), 6));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateScheduledExit {
                exit_allowed_era: 2,
                candidate: 6,
                scheduled_exit: 4
            }));
            roll_to(21);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(6), 6, 0));
            assert_ok!(ParachainStaking::join_candidates(Origin::signed(6), 69u128, 100u32));
            assert_last_event!(MetaEvent::ParachainStaking(Event::JoinedCollatorCandidates {
                account: 6,
                amount_locked: 69u128,
                new_total_amt_locked: 469u128,
            }));
            roll_to(27);
            // should choose top TotalSelectedCandidates (5), in order
            let expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CandidateScheduledExit {
                    exit_allowed_era: 2,
                    candidate: 6,
                    scheduled_exit: 4,
                },
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 3, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 4, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 4, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 4, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 5, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 5, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 5, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CandidateLeft {
                    ex_candidate: 6,
                    unlocked_amount: 50,
                    new_total_amt_locked: 400,
                },
                Event::JoinedCollatorCandidates {
                    account: 6,
                    amount_locked: 69,
                    new_total_amt_locked: 469,
                },
                Event::CollatorChosen { era: 6, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 6, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 6, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 6, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 6, collator_account: 6, total_exposed_amount: 69 },
                Event::NewEra {
                    starting_block: 25,
                    era: 6,
                    selected_collators_number: 5,
                    total_balance: 409,
                },
            ];
            assert_eq_events!(expected);
        });
}

#[test]
fn payout_distribution_to_solo_collators() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 1000),
            (2, 1000),
            (3, 1000),
            (4, 1000),
            (5, 1000),
            (6, 1000),
            (7, 33),
            (8, 33),
            (9, 33),
        ])
        .with_candidates(vec![(1, 100), (2, 90), (3, 80), (4, 70), (5, 60), (6, 50)])
        .build()
        .execute_with(|| {
            roll_to(8);
            // should choose top TotalCandidatesSelected (5), in order
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
            ];
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for all blocks this era
            set_author(2, 1, 100);
            set_reward_pot(305);
            roll_to(16);
            // pay total issuance to 1
            let mut new = vec![
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 3, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 4, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 4, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 4, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::Rewarded { account: 1, rewards: 305 },
            ];
            expected.append(&mut new);
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for 3 blocks this era
            set_author(4, 1, 60);
            // ~ set block author as 2 for 2 blocks this era
            set_author(4, 2, 40);
            set_reward_pot(320);
            roll_to(26);
            // pay 60% total issuance to 1 and 40% total issuance to 2
            let mut new1 = vec![
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 5, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 5, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 5, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CollatorChosen { era: 6, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 6, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 6, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 6, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 6, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 25,
                    era: 6,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::Rewarded { account: 1, rewards: 192 },
                Event::Rewarded { account: 2, rewards: 128 },
            ];
            expected.append(&mut new1);
            assert_eq_events!(expected.clone());
            // ~ each collator produces 1 block this era
            set_author(6, 1, 20);
            set_author(6, 2, 20);
            set_author(6, 3, 20);
            set_author(6, 4, 20);
            set_author(6, 5, 20);
            set_reward_pot(336);
            roll_to(39);
            // pay 20% issuance for all collators
            let mut new2 = vec![
                Event::CollatorChosen { era: 7, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 7, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 7, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 7, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 7, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 30,
                    era: 7,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::CollatorChosen { era: 8, collator_account: 1, total_exposed_amount: 100 },
                Event::CollatorChosen { era: 8, collator_account: 2, total_exposed_amount: 90 },
                Event::CollatorChosen { era: 8, collator_account: 3, total_exposed_amount: 80 },
                Event::CollatorChosen { era: 8, collator_account: 4, total_exposed_amount: 70 },
                Event::CollatorChosen { era: 8, collator_account: 5, total_exposed_amount: 60 },
                Event::NewEra {
                    starting_block: 35,
                    era: 8,
                    selected_collators_number: 5,
                    total_balance: 400,
                },
                Event::Rewarded { account: 5, rewards: 67 },
                Event::Rewarded { account: 3, rewards: 67 },
                Event::Rewarded { account: 4, rewards: 67 },
                Event::Rewarded { account: 1, rewards: 67 },
                Event::Rewarded { account: 2, rewards: 67 },
            ];
            expected.append(&mut new2);
            assert_eq_events!(expected);
            // check that distributing rewards clears awarded pts
            assert!(ParachainStaking::awarded_pts(1, 1).is_zero());
            assert!(ParachainStaking::awarded_pts(4, 1).is_zero());
            assert!(ParachainStaking::awarded_pts(4, 2).is_zero());
            assert!(ParachainStaking::awarded_pts(6, 1).is_zero());
            assert!(ParachainStaking::awarded_pts(6, 2).is_zero());
            assert!(ParachainStaking::awarded_pts(6, 3).is_zero());
            assert!(ParachainStaking::awarded_pts(6, 4).is_zero());
            assert!(ParachainStaking::awarded_pts(6, 5).is_zero());
        });
}

#[test]
fn multiple_nominations() {
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
            roll_to(8);
            // chooses top TotalSelectedCandidates (5), in order
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
            ];
            assert_eq_events!(expected.clone());
            assert_ok!(ParachainStaking::nominate(Origin::signed(6), 2, 10, 10, 10));
            assert_ok!(ParachainStaking::nominate(Origin::signed(6), 3, 10, 10, 10));
            assert_ok!(ParachainStaking::nominate(Origin::signed(6), 4, 10, 10, 10));
            roll_to(16);
            let mut new = vec![
                Event::Nomination {
                    nominator: 6,
                    locked_amount: 10,
                    candidate: 2,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 50 },
                },
                Event::Nomination {
                    nominator: 6,
                    locked_amount: 10,
                    candidate: 3,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 30 },
                },
                Event::Nomination {
                    nominator: 6,
                    locked_amount: 10,
                    candidate: 4,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 30 },
                },
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 3, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 5,
                    total_balance: 170,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 4, collator_account: 2, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 4, collator_account: 3, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 4, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 5,
                    total_balance: 170,
                },
            ];
            expected.append(&mut new);
            assert_eq_events!(expected.clone());
            roll_to(21);
            assert_ok!(ParachainStaking::nominate(Origin::signed(7), 2, 80, 10, 10));
            assert_ok!(ParachainStaking::nominate(Origin::signed(10), 2, 10, 10, 10),);
            roll_to(26);
            let mut new2 = vec![
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 5, collator_account: 2, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 5, collator_account: 3, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 5, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 5,
                    total_balance: 170,
                },
                Event::Nomination {
                    nominator: 7,
                    locked_amount: 80,
                    candidate: 2,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 130 },
                },
                Event::Nomination {
                    nominator: 10,
                    locked_amount: 10,
                    candidate: 2,
                    nominator_position: NominatorAdded::AddedToBottom,
                },
                Event::CollatorChosen { era: 6, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 6, collator_account: 2, total_exposed_amount: 130 },
                Event::CollatorChosen { era: 6, collator_account: 3, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 6, collator_account: 4, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 6, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 25,
                    era: 6,
                    selected_collators_number: 5,
                    total_balance: 250,
                },
            ];
            expected.append(&mut new2);
            assert_eq_events!(expected.clone());
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(2), 5));
            assert_last_event!(MetaEvent::ParachainStaking(Event::CandidateScheduledExit {
                exit_allowed_era: 6,
                candidate: 2,
                scheduled_exit: 8
            }));
            roll_to(31);
            let mut new3 = vec![
                Event::CandidateScheduledExit {
                    exit_allowed_era: 6,
                    candidate: 2,
                    scheduled_exit: 8,
                },
                Event::CollatorChosen { era: 7, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 7, collator_account: 3, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 7, collator_account: 4, total_exposed_amount: 30 },
                Event::CollatorChosen { era: 7, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 30,
                    era: 7,
                    selected_collators_number: 4,
                    total_balance: 120,
                },
            ];
            expected.append(&mut new3);
            assert_eq_events!(expected);
            // verify that nominations are removed after collator leaves, not before
            assert_eq!(ParachainStaking::nominator_state(7).unwrap().total(), 90);
            assert_eq!(ParachainStaking::nominator_state(7).unwrap().nominations.0.len(), 2usize);
            assert_eq!(ParachainStaking::nominator_state(6).unwrap().total(), 40);
            assert_eq!(ParachainStaking::nominator_state(6).unwrap().nominations.0.len(), 4usize);
            assert_eq!(Balances::locks(&6)[0].amount, 40);
            assert_eq!(Balances::locks(&7)[0].amount, 90);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&6), 60);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&7), 10);
            roll_to(40);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(2), 2, 5));
            assert_eq!(ParachainStaking::nominator_state(7).unwrap().total(), 10);
            assert_eq!(ParachainStaking::nominator_state(6).unwrap().total(), 30);
            assert_eq!(ParachainStaking::nominator_state(7).unwrap().nominations.0.len(), 1usize);
            assert_eq!(ParachainStaking::nominator_state(6).unwrap().nominations.0.len(), 3usize);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&6), 70);
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&7), 90);
        });
}

#[test]
// The test verifies that the pending revoke request is removed by 2's exit so there is no dangling
// revoke request after 2 exits
fn execute_leave_candidate_removes_nominations() {
    ExtBuilder::default()
        .with_balances(vec![(1, 100), (2, 100), (3, 100), (4, 100)])
        .with_candidates(vec![(1, 20), (2, 20)])
        .with_nominations(vec![(3, 1, 10), (3, 2, 10), (4, 1, 10), (4, 2, 10)])
        .build()
        .execute_with(|| {
            // Verifies the revocation request is initially empty
            assert!(!ParachainStaking::nomination_scheduled_requests(&2)
                .iter()
                .any(|x| x.nominator == 3));

            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(2), 2));
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(3), 2));
            // Verifies the revocation request is present
            assert!(ParachainStaking::nomination_scheduled_requests(&2)
                .iter()
                .any(|x| x.nominator == 3));

            roll_to(16);
            assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(2), 2, 2));
            // Verifies the revocation request is again empty
            assert!(!ParachainStaking::nomination_scheduled_requests(&2)
                .iter()
                .any(|x| x.nominator == 3));
        });
}

#[test]
fn payouts_follow_nomination_changes() {
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
            roll_to(8);
            // chooses top TotalSelectedCandidates (5), in order
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
            ];
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for all blocks this era
            set_author(2, 1, 100);
            set_reward_pot(50);
            roll_to(16);
            // distribute total reward to collator 1 and its nominators 6, 7, 19
            let mut new = vec![
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::CollatorChosen { era: 4, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 4, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 4, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 4, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 4, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 15,
                    era: 4,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 20 },
                Event::Rewarded { account: 6, rewards: 10 },
                Event::Rewarded { account: 7, rewards: 10 },
                Event::Rewarded { account: 10, rewards: 10 },
            ];
            expected.append(&mut new);
            assert_eq_events!(expected.clone());
            // ~ set block author as 1 for all blocks this era
            set_author(3, 1, 100);
            set_author(4, 1, 100);
            set_author(5, 1, 100);
            set_author(6, 1, 100);
            // 1. ensure nominators are paid for 2 eras after they leave
            assert_noop!(
                ParachainStaking::schedule_leave_nominators(Origin::signed(66)),
                Error::<Test>::NominatorDNE
            );
            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(6)));
            // fast forward to block in which nominator 6 exit executes
            set_reward_pot(52);
            roll_to(20);

            set_reward_pot(56);
            roll_to(25);

            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(6), 6, 10));
            // keep paying 6 (note: inflation is in terms of total issuance so that's why 1 is 21)
            let mut new2 = vec![
                Event::NominatorExitScheduled { era: 4, nominator: 6, scheduled_exit: 6 },
                Event::CollatorChosen { era: 5, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 5, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 5, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 5, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 5, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 20,
                    era: 5,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 21 },
                Event::Rewarded { account: 6, rewards: 10 },
                Event::Rewarded { account: 7, rewards: 10 },
                Event::Rewarded { account: 10, rewards: 10 },
                Event::CollatorChosen { era: 6, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 6, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 6, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 6, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 6, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 25,
                    era: 6,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 22 },
                Event::Rewarded { account: 6, rewards: 11 },
                Event::Rewarded { account: 7, rewards: 11 },
                Event::Rewarded { account: 10, rewards: 11 },
                Event::NominatorLeftCandidate {
                    nominator: 6,
                    candidate: 1,
                    unstaked_amount: 10,
                    total_candidate_staked: 40,
                },
                Event::NominatorLeft { nominator: 6, unstaked_amount: 10 },
            ];
            expected.append(&mut new2);
            assert_eq_events!(expected.clone());
            // 6 won't be paid for this era because they left already
            set_author(7, 1, 100);
            set_reward_pot(58);
            roll_to(30);
            set_reward_pot(61);
            roll_to(35);
            // keep paying 6
            let mut new3 = vec![
                Event::CollatorChosen { era: 7, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 7, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 7, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 7, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 7, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 30,
                    era: 7,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 29 },
                Event::Rewarded { account: 7, rewards: 14 },
                Event::Rewarded { account: 10, rewards: 14 },
                Event::CollatorChosen { era: 8, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 8, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 8, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 8, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 8, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 35,
                    era: 8,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 30 },
                Event::Rewarded { account: 7, rewards: 15 },
                Event::Rewarded { account: 10, rewards: 15 },
            ];
            expected.append(&mut new3);
            assert_eq_events!(expected.clone());
            set_author(8, 1, 100);
            set_reward_pot(64);
            roll_to(40);
            // no more paying 6
            let mut new4 = vec![
                Event::CollatorChosen { era: 9, collator_account: 1, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 9, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 9, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 9, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 9, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 40,
                    era: 9,
                    selected_collators_number: 5,
                    total_balance: 130,
                },
                Event::Rewarded { account: 1, rewards: 32 },
                Event::Rewarded { account: 7, rewards: 16 },
                Event::Rewarded { account: 10, rewards: 16 },
            ];
            expected.append(&mut new4);
            assert_eq_events!(expected.clone());
            set_author(9, 1, 100);
            assert_ok!(ParachainStaking::nominate(Origin::signed(8), 1, 10, 10, 10));
            set_reward_pot(67);
            roll_to(45);
            // new nomination is not rewarded yet
            let mut new5 = vec![
                Event::Nomination {
                    nominator: 8,
                    locked_amount: 10,
                    candidate: 1,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 50 },
                },
                Event::CollatorChosen { era: 10, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 10, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 10, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 10, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 10, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 45,
                    era: 10,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 33 },
                Event::Rewarded { account: 7, rewards: 17 },
                Event::Rewarded { account: 10, rewards: 17 },
            ];
            expected.append(&mut new5);
            assert_eq_events!(expected.clone());
            set_author(10, 1, 100);
            set_reward_pot(70);
            roll_to(50);
            // new nomination not rewarded yet
            let mut new6 = vec![
                Event::CollatorChosen { era: 11, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 11, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 11, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 11, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 11, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 50,
                    era: 11,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 35 },
                Event::Rewarded { account: 7, rewards: 17 },
                Event::Rewarded { account: 10, rewards: 17 },
            ];
            expected.append(&mut new6);
            assert_eq_events!(expected.clone());
            set_reward_pot(75);
            roll_to(55);
            // new nomination is rewarded for first time
            // 2 eras after joining (`RewardPaymentDelay` = 2)
            let mut new7 = vec![
                Event::CollatorChosen { era: 12, collator_account: 1, total_exposed_amount: 50 },
                Event::CollatorChosen { era: 12, collator_account: 2, total_exposed_amount: 40 },
                Event::CollatorChosen { era: 12, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 12, collator_account: 4, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 12, collator_account: 5, total_exposed_amount: 10 },
                Event::NewEra {
                    starting_block: 55,
                    era: 12,
                    selected_collators_number: 5,
                    total_balance: 140,
                },
                Event::Rewarded { account: 1, rewards: 30 },
                Event::Rewarded { account: 7, rewards: 15 },
                Event::Rewarded { account: 10, rewards: 15 },
                Event::Rewarded { account: 8, rewards: 15 },
            ];
            expected.append(&mut new7);
            assert_eq_events!(expected);
        });
}

#[test]
fn bottom_nominations_are_empty_when_top_nominations_not_full() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 10), (3, 10), (4, 10), (5, 10)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // no top nominators => no bottom nominators
            let top_nominations = ParachainStaking::top_nominations(1).unwrap();
            let bottom_nominations = ParachainStaking::bottom_nominations(1).unwrap();
            assert!(top_nominations.nominations.is_empty());
            assert!(bottom_nominations.nominations.is_empty());
            // 1 nominator => 1 top nominator, 0 bottom nominators
            assert_ok!(ParachainStaking::nominate(Origin::signed(2), 1, 10, 10, 10));
            let top_nominations = ParachainStaking::top_nominations(1).unwrap();
            let bottom_nominations = ParachainStaking::bottom_nominations(1).unwrap();
            assert_eq!(top_nominations.nominations.len(), 1usize);
            assert!(bottom_nominations.nominations.is_empty());
            // 2 nominators => 2 top nominators, 0 bottom nominators
            assert_ok!(ParachainStaking::nominate(Origin::signed(3), 1, 10, 10, 10));
            let top_nominations = ParachainStaking::top_nominations(1).unwrap();
            let bottom_nominations = ParachainStaking::bottom_nominations(1).unwrap();
            assert_eq!(top_nominations.nominations.len(), 2usize);
            assert!(bottom_nominations.nominations.is_empty());
            // 3 nominators => 3 top nominators, 0 bottom nominators
            assert_ok!(ParachainStaking::nominate(Origin::signed(4), 1, 10, 10, 10));
            let top_nominations = ParachainStaking::top_nominations(1).unwrap();
            let bottom_nominations = ParachainStaking::bottom_nominations(1).unwrap();
            assert_eq!(top_nominations.nominations.len(), 3usize);
            assert!(bottom_nominations.nominations.is_empty());
            // 4 nominators => 4 top nominators, 0 bottom nominators
            assert_ok!(ParachainStaking::nominate(Origin::signed(5), 1, 10, 10, 10));
            let top_nominations = ParachainStaking::top_nominations(1).unwrap();
            let bottom_nominations = ParachainStaking::bottom_nominations(1).unwrap();
            assert_eq!(top_nominations.nominations.len(), 4usize);
            assert!(bottom_nominations.nominations.is_empty());
        });
}

#[test]
fn candidate_pool_updates_when_total_counted_changes() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (3, 19),
            (4, 20),
            (5, 21),
            (6, 22),
            (7, 15),
            (8, 16),
            (9, 17),
            (10, 18),
        ])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![
            (3, 1, 11),
            (4, 1, 12),
            (5, 1, 13),
            (6, 1, 14),
            (7, 1, 15),
            (8, 1, 16),
            (9, 1, 17),
            (10, 1, 18),
        ])
        .build()
        .execute_with(|| {
            fn is_candidate_pool_bond(account: u64, bond: u128) {
                let pool = ParachainStaking::candidate_pool();
                for candidate in pool.0 {
                    if candidate.owner == account {
                        assert_eq!(
                            candidate.amount, bond,
                            "Candidate Bond {:?} is Not Equal to Expected: {:?}",
                            candidate.amount, bond
                        );
                    }
                }
            }
            // 15 + 16 + 17 + 18 + 20 = 86 (top 4 + self bond)
            is_candidate_pool_bond(1, 86);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(3), 1, 8));
            // 3: 11 -> 19 => 3 is in top, bumps out 7
            // 16 + 17 + 18 + 19 + 20 = 90 (top 4 + self bond)
            is_candidate_pool_bond(1, 90);
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(4), 1, 8));
            // 4: 12 -> 20 => 4 is in top, bumps out 8
            // 17 + 18 + 19 + 20 + 20 = 94 (top 4 + self bond)
            is_candidate_pool_bond(1, 94);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(10), 1, 3));
            roll_to(30);
            // 10: 18 -> 15 => 10 bumped to bottom, 8 bumped to top (- 18 + 16 = -2 for count)
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(10), 10, 1));
            // 16 + 17 + 19 + 20 + 20 = 92 (top 4 + self bond)
            is_candidate_pool_bond(1, 92);
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(9), 1, 4));
            roll_to(40);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(9), 9, 1));
            // 15 + 16 + 19 + 20 + 20 = 90 (top 4 + self bond)
            is_candidate_pool_bond(1, 90);
        });
}

#[test]
fn only_top_collators_are_counted() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 20),
            (3, 19),
            (4, 20),
            (5, 21),
            (6, 22),
            (7, 15),
            (8, 16),
            (9, 17),
            (10, 18),
        ])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![
            (3, 1, 11),
            (4, 1, 12),
            (5, 1, 13),
            (6, 1, 14),
            (7, 1, 15),
            (8, 1, 16),
            (9, 1, 17),
            (10, 1, 18),
        ])
        .build()
        .execute_with(|| {
            // sanity check that 3-10 are nominators immediately
            for i in 3..11 {
                assert!(ParachainStaking::is_nominator(&i));
            }
            let collator_state = ParachainStaking::candidate_info(1).unwrap();
            // 15 + 16 + 17 + 18 + 20 = 86 (top 4 + self bond)
            assert_eq!(collator_state.total_counted, 86);
            // bump bottom to the top
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(3), 1, 8));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 3,
                candidate: 1,
                amount: 8,
                in_top: true,
            });
            let collator_state = ParachainStaking::candidate_info(1).unwrap();
            // 16 + 17 + 18 + 19 + 20 = 90 (top 4 + self bond)
            assert_eq!(collator_state.total_counted, 90);
            // bump bottom to the top
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(4), 1, 8));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 4,
                candidate: 1,
                amount: 8,
                in_top: true,
            });
            let collator_state = ParachainStaking::candidate_info(1).unwrap();
            // 17 + 18 + 19 + 20 + 20 = 94 (top 4 + self bond)
            assert_eq!(collator_state.total_counted, 94);
            // bump bottom to the top
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(5), 1, 8));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 5,
                candidate: 1,
                amount: 8,
                in_top: true,
            });
            let collator_state = ParachainStaking::candidate_info(1).unwrap();
            // 18 + 19 + 20 + 21 + 20 = 98 (top 4 + self bond)
            assert_eq!(collator_state.total_counted, 98);
            // bump bottom to the top
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(6), 1, 8));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 6,
                candidate: 1,
                amount: 8,
                in_top: true,
            });
            let collator_state = ParachainStaking::candidate_info(1).unwrap();
            // 19 + 20 + 21 + 22 + 20 = 102 (top 4 + self bond)
            assert_eq!(collator_state.total_counted, 102);
        });
}

#[test]
fn nomination_events_convey_correct_position() {
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
        .with_candidates(vec![(1, 20), (2, 20)])
        .with_nominations(vec![(3, 1, 11), (4, 1, 12), (5, 1, 13), (6, 1, 14)])
        .build()
        .execute_with(|| {
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 11 + 12 + 13 + 14 + 20 = 70 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 70);
            // Top nominations are full, new highest nomination is made
            assert_ok!(ParachainStaking::nominate(Origin::signed(7), 1, 15, 10, 10));
            assert_event_emitted!(Event::Nomination {
                nominator: 7,
                locked_amount: 15,
                candidate: 1,
                nominator_position: NominatorAdded::AddedToTop { new_total: 74 },
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 12 + 13 + 14 + 15 + 20 = 70 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 74);
            // New nomination is added to the bottom
            assert_ok!(ParachainStaking::nominate(Origin::signed(8), 1, 10, 10, 10));
            assert_event_emitted!(Event::Nomination {
                nominator: 8,
                locked_amount: 10,
                candidate: 1,
                nominator_position: NominatorAdded::AddedToBottom,
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 12 + 13 + 14 + 15 + 20 = 70 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 74);
            // 8 increases nomination to the top
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(8), 1, 3));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 8,
                candidate: 1,
                amount: 3,
                in_top: true,
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 13 + 13 + 14 + 15 + 20 = 75 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 75);
            // 3 increases nomination but stays in bottom
            assert_ok!(ParachainStaking::nominator_bond_more(Origin::signed(3), 1, 1));
            assert_event_emitted!(Event::NominationIncreased {
                nominator: 3,
                candidate: 1,
                amount: 1,
                in_top: false,
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 13 + 13 + 14 + 15 + 20 = 75 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 75);
            // 6 decreases nomination but stays in top
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(6), 1, 2));
            assert_event_emitted!(Event::NominationDecreaseScheduled {
                nominator: 6,
                candidate: 1,
                amount_to_decrease: 2,
                execute_era: 3,
            });
            roll_to(30);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(6), 6, 1));
            assert_event_emitted!(Event::NominationDecreased {
                nominator: 6,
                candidate: 1,
                amount: 2,
                in_top: true,
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 12 + 13 + 13 + 15 + 20 = 73 (top 4 + self bond)ƒ
            assert_eq!(collator1_state.total_counted, 73);
            // 6 decreases nomination and is bumped to bottom
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(6), 1, 1));
            assert_event_emitted!(Event::NominationDecreaseScheduled {
                nominator: 6,
                candidate: 1,
                amount_to_decrease: 1,
                execute_era: 9,
            });
            roll_to(40);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(6), 6, 1));
            assert_event_emitted!(Event::NominationDecreased {
                nominator: 6,
                candidate: 1,
                amount: 1,
                in_top: false,
            });
            let collator1_state = ParachainStaking::candidate_info(1).unwrap();
            // 12 + 13 + 13 + 15 + 20 = 73 (top 4 + self bond)
            assert_eq!(collator1_state.total_counted, 73);
        });
}

#[test]
fn no_rewards_paid_until_after_reward_payment_delay() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (2, 20), (3, 20), (4, 20)])
        .build()
        .execute_with(|| {
            roll_to_era_begin(2);
            // payouts for era 1
            set_author(1, 1, 1);
            set_author(1, 2, 1);
            set_author(1, 3, 1);
            set_author(1, 4, 1);
            set_author(1, 4, 1);
            let mut expected = vec![
                Event::CollatorChosen { era: 2, collator_account: 1, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 2, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 2, collator_account: 4, total_exposed_amount: 20 },
                Event::NewEra {
                    starting_block: 5,
                    era: 2,
                    selected_collators_number: 4,
                    total_balance: 80,
                },
            ];
            assert_eq_events!(expected);

            set_reward_pot(5);
            roll_to_era_begin(3);
            expected.append(&mut vec![
                Event::CollatorChosen { era: 3, collator_account: 1, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 2, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 3, total_exposed_amount: 20 },
                Event::CollatorChosen { era: 3, collator_account: 4, total_exposed_amount: 20 },
                Event::NewEra {
                    starting_block: 10,
                    era: 3,
                    selected_collators_number: 4,
                    total_balance: 80,
                },
                // rewards will begin immediately following a NewEra
                Event::Rewarded { account: 3, rewards: 1 },
            ]);
            assert_eq_events!(expected);

            // roll to the next block where we start era 3; we should have era change and first
            // payout made.
            roll_one_block();
            expected.push(Event::Rewarded { account: 4, rewards: 2 });
            assert_eq_events!(expected);

            roll_one_block();
            expected.push(Event::Rewarded { account: 1, rewards: 1 });
            assert_eq_events!(expected);

            roll_one_block();
            expected.push(Event::Rewarded { account: 2, rewards: 1 });
            assert_eq_events!(expected);

            // there should be no more payments in this era...
            let num_blocks_rolled = roll_to_era_end(3);
            assert_eq_events!(expected);
            assert_eq!(num_blocks_rolled, 1);
        });
}

#[test]
fn deferred_payment_storage_items_are_cleaned_up() {
    use crate::*;

    // this test sets up two collators, gives them points in era one, and focuses on the
    // storage over the next several blocks to show that it is properly cleaned up

    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 20)])
        .with_candidates(vec![(1, 20), (2, 20)])
        .build()
        .execute_with(|| {
            let mut era: u32 = 1;
            set_author(era, 1, 1);
            set_author(era, 2, 1);

            // reflects genesis?
            assert!(<AtStake<Test>>::contains_key(era, 1));
            assert!(<AtStake<Test>>::contains_key(era, 2));

            era = 2;
            roll_to_era_begin(era.into());
            let mut expected = vec![
                Event::CollatorChosen { era, collator_account: 1, total_exposed_amount: 20 },
                Event::CollatorChosen { era, collator_account: 2, total_exposed_amount: 20 },
                Event::NewEra {
                    starting_block: 5,
                    era,
                    selected_collators_number: 2,
                    total_balance: 40,
                },
            ];
            assert_eq_events!(expected);

            // we should have AtStake snapshots as soon as we start a era...
            assert!(<AtStake<Test>>::contains_key(2, 1));
            assert!(<AtStake<Test>>::contains_key(2, 2));
            // ...and it should persist until the era is fully paid out
            assert!(<AtStake<Test>>::contains_key(1, 1));
            assert!(<AtStake<Test>>::contains_key(1, 2));

            assert!(
                !<DelayedPayouts<Test>>::contains_key(1),
                "DelayedPayouts shouldn't be populated until after RewardPaymentDelay"
            );
            assert!(
                <Points<Test>>::contains_key(1),
                "Points should be populated during current era"
            );
            assert!(<Staked<Test>>::contains_key(1), "Staked should be populated when era changes");

            assert!(
                !<Points<Test>>::contains_key(2),
                "Points should not be populated until author noted"
            );
            assert!(<Staked<Test>>::contains_key(2), "Staked should be populated when era changes");

            // first payout occurs in era 3
            era = 3;
            set_reward_pot(3);
            roll_to_era_begin(era.into());
            expected.append(&mut vec![
                Event::CollatorChosen { era, collator_account: 1, total_exposed_amount: 20 },
                Event::CollatorChosen { era, collator_account: 2, total_exposed_amount: 20 },
                Event::NewEra {
                    starting_block: 10,
                    era,
                    selected_collators_number: 2,
                    total_balance: 40,
                },
                Event::Rewarded { account: 1, rewards: 1 },
            ]);
            assert_eq_events!(expected);

            // payouts should exist for past eras that haven't been paid out yet..
            assert!(<AtStake<Test>>::contains_key(3, 1));
            assert!(<AtStake<Test>>::contains_key(3, 2));
            assert!(<AtStake<Test>>::contains_key(2, 1));
            assert!(<AtStake<Test>>::contains_key(2, 2));

            assert!(
                <DelayedPayouts<Test>>::contains_key(1),
                "DelayedPayouts should be populated after RewardPaymentDelay"
            );
            assert!(<Points<Test>>::contains_key(1));
            assert!(
                !<Staked<Test>>::contains_key(1),
                "Staked should be cleaned up after era change"
            );

            assert!(!<DelayedPayouts<Test>>::contains_key(2));
            assert!(!<Points<Test>>::contains_key(2), "We never rewarded points for era 2");
            assert!(<Staked<Test>>::contains_key(2));

            assert!(!<DelayedPayouts<Test>>::contains_key(3));
            assert!(!<Points<Test>>::contains_key(3), "We never awarded points for era 3");
            assert!(<Staked<Test>>::contains_key(3));

            // collator 1 has been paid in this last block and associated storage cleaned up
            assert!(!<AtStake<Test>>::contains_key(1, 1));
            assert!(!<AwardedPts<Test>>::contains_key(1, 1));

            // but collator 2 hasn't been paid
            assert!(<AtStake<Test>>::contains_key(1, 2));
            assert!(<AwardedPts<Test>>::contains_key(1, 2));

            era = 4;
            roll_to_era_begin(era.into());
            expected.append(&mut vec![
                Event::Rewarded { account: 2, rewards: 1 }, // from previous era
                Event::CollatorChosen { era, collator_account: 1, total_exposed_amount: 20 },
                Event::CollatorChosen { era, collator_account: 2, total_exposed_amount: 20 },
                Event::NewEra {
                    starting_block: 15,
                    era,
                    selected_collators_number: 2,
                    total_balance: 40,
                },
            ]);
            assert_eq_events!(expected);

            // collators have both been paid and storage fully cleaned up for era 1
            assert!(!<AtStake<Test>>::contains_key(1, 2));
            assert!(!<AwardedPts<Test>>::contains_key(1, 2));
            assert!(!<Staked<Test>>::contains_key(1));
            assert!(!<Points<Test>>::contains_key(1)); // points should be cleaned up
            assert!(!<DelayedPayouts<Test>>::contains_key(1));

            roll_to_era_end(4);

            // no more events expected
            assert_eq_events!(expected);
        });
}

#[test]
fn deferred_payment_steady_state_event_flow() {
    use frame_support::traits::{Currency, ExistenceRequirement, WithdrawReasons};

    // this test "flows" through a number of eras, asserting that certain things do/don't happen
    // once the staking pallet is in a "steady state" (specifically, once we are past the first few
    // eras to clear RewardPaymentDelay)

    ExtBuilder::default()
        .with_balances(vec![
            // collators
            (1, 200),
            (2, 200),
            (3, 200),
            (4, 200),
            // nominators
            (11, 200),
            (22, 200),
            (33, 200),
            (44, 200),
            // burn account, see `reset_issuance()`
            (111, 1000),
        ])
        .with_candidates(vec![(1, 200), (2, 200), (3, 200), (4, 200)])
        .with_nominations(vec![
            // nominator 11 nominates 100 to 1 and 2
            (11, 1, 100),
            (11, 2, 100),
            // nominator 22 nominates 100 to 2 and 3
            (22, 2, 100),
            (22, 3, 100),
            // nominator 33 nominates 100 to 3 and 4
            (33, 3, 100),
            (33, 4, 100),
            // nominator 44 nominates 100 to 4 and 1
            (44, 4, 100),
            (44, 1, 100),
        ])
        .build()
        .execute_with(|| {
            // convenience to set the era points consistently
            let set_era_points = |era: u64| {
                set_author(era as u32, 1, 1);
                set_author(era as u32, 2, 1);
                set_author(era as u32, 3, 1);
                set_author(era as u32, 4, 1);
            };

            // grab initial issuance -- we will reset it before era issuance is calculated so that
            // it is consistent every era
            let initial_issuance = Balances::total_issuance();
            let reset_issuance = || {
                let new_issuance = Balances::total_issuance();
                let diff = new_issuance - initial_issuance;
                let burned = Balances::burn(diff);
                Balances::settle(
                    &111,
                    burned,
                    WithdrawReasons::FEE,
                    ExistenceRequirement::AllowDeath,
                )
                .expect("Account can absorb burn");
            };

            // fn to roll through the first RewardPaymentDelay eras. returns new era index
            let roll_through_initial_eras = |mut era: u64| -> u64 {
                while era < crate::mock::RewardPaymentDelay::get() as u64 + 1 {
                    set_era_points(era);

                    roll_to_era_end(era);
                    era += 1;
                }

                reset_issuance();

                era
            };

            // roll through a "steady state" era and make all of our assertions
            // returns new era index
            let roll_through_steady_state_era = |era: u64| -> u64 {
                set_reward_pot(130);
                let num_eras_rolled = roll_to_era_begin(era);
                assert_eq!(num_eras_rolled, 1, "expected to be at era begin already");

                let expected = vec![
                    Event::CollatorChosen {
                        era: era as u32,
                        collator_account: 1,
                        total_exposed_amount: 400,
                    },
                    Event::CollatorChosen {
                        era: era as u32,
                        collator_account: 2,
                        total_exposed_amount: 400,
                    },
                    Event::CollatorChosen {
                        era: era as u32,
                        collator_account: 3,
                        total_exposed_amount: 400,
                    },
                    Event::CollatorChosen {
                        era: era as u32,
                        collator_account: 4,
                        total_exposed_amount: 400,
                    },
                    Event::NewEra {
                        starting_block: (era - 1) * 5,
                        era: era as u32,
                        selected_collators_number: 4,
                        total_balance: 1600,
                    },
                    // first payout should occur on era change
                    Event::Rewarded { account: 3, rewards: 16 },
                    Event::Rewarded { account: 22, rewards: 8 },
                    Event::Rewarded { account: 33, rewards: 8 },
                ];
                assert_eq_last_events!(expected);

                set_era_points(era);

                roll_one_block();
                let expected = vec![
                    Event::Rewarded { account: 4, rewards: 16 },
                    Event::Rewarded { account: 33, rewards: 8 },
                    Event::Rewarded { account: 44, rewards: 8 },
                ];
                assert_eq_last_events!(expected);

                roll_one_block();
                let expected = vec![
                    Event::Rewarded { account: 1, rewards: 16 },
                    Event::Rewarded { account: 11, rewards: 8 },
                    Event::Rewarded { account: 44, rewards: 8 },
                ];
                assert_eq_last_events!(expected);

                roll_one_block();
                let expected = vec![
                    Event::Rewarded { account: 2, rewards: 16 },
                    Event::Rewarded { account: 11, rewards: 8 },
                    Event::Rewarded { account: 22, rewards: 8 },
                ];
                assert_eq_last_events!(expected);

                roll_one_block();
                let expected = vec![
                    // we paid everyone out by now, should repeat last event
                    Event::Rewarded { account: 22, rewards: 8 },
                ];
                assert_eq_last_events!(expected);

                let num_eras_rolled = roll_to_era_end(era);
                assert_eq!(num_eras_rolled, 0, "expected to be at era end already");

                reset_issuance();

                era + 1
            };

            let mut era = 1;
            era = roll_through_initial_eras(era); // we should be at RewardPaymentDelay
            for _ in 1..5 {
                era = roll_through_steady_state_era(era);
            }
        });
}

#[test]
fn nomination_kicked_from_bottom_removes_pending_request() {
    ExtBuilder::default()
        .with_balances(vec![
            (1, 30),
            (2, 29),
            (3, 20),
            (4, 20),
            (5, 20),
            (6, 20),
            (7, 20),
            (8, 20),
            (9, 20),
            (10, 20),
            (11, 30),
        ])
        .with_candidates(vec![(1, 30), (11, 30)])
        .with_nominations(vec![
            (2, 1, 19),
            (2, 11, 10), // second nomination so not left after first is kicked
            (3, 1, 20),
            (4, 1, 20),
            (5, 1, 20),
            (6, 1, 20),
            (7, 1, 20),
            (8, 1, 20),
            (9, 1, 20),
        ])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            // 10 nominates to full 1 => kicks lowest nomination (2, 19)
            assert_ok!(ParachainStaking::nominate(Origin::signed(10), 1, 20, 8, 0));
            // check the event
            assert_event_emitted!(Event::NominationKicked {
                nominator: 2,
                candidate: 1,
                unstaked_amount: 19,
            });
            // ensure request DNE
            assert!(!ParachainStaking::nomination_scheduled_requests(&1)
                .iter()
                .any(|x| x.nominator == 2));
        });
}

#[test]
fn no_selected_candidates_defaults_to_last_era_collators() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 30), (3, 30), (4, 30), (5, 30)])
        .with_candidates(vec![(1, 30), (2, 30), (3, 30), (4, 30), (5, 30)])
        .build()
        .execute_with(|| {
            roll_to_era_begin(1);
            // schedule to leave
            for i in 1..6 {
                assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(i), 5));
            }
            let old_era = ParachainStaking::era().current;
            let old_selected_candidates = ParachainStaking::selected_candidates();
            let mut old_at_stake_snapshots = Vec::new();
            for account in old_selected_candidates.clone() {
                old_at_stake_snapshots.push(<AtStake<Test>>::get(old_era, account));
            }
            roll_to_era_begin(3);
            // execute leave
            for i in 1..6 {
                assert_ok!(ParachainStaking::execute_leave_candidates(Origin::signed(i), i, 0,));
            }
            // next era
            roll_to_era_begin(4);
            let new_era = ParachainStaking::era().current;
            // check AtStake matches previous
            let new_selected_candidates = ParachainStaking::selected_candidates();
            assert_eq!(old_selected_candidates, new_selected_candidates);
            let mut index = 0usize;
            for account in new_selected_candidates {
                assert_eq!(old_at_stake_snapshots[index], <AtStake<Test>>::get(new_era, account));
                index += 1usize;
            }
        });
}

#[test]
fn test_nominator_scheduled_for_revoke_is_rewarded_for_previous_eras_but_not_for_future() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 1, 2 and 3
            (1..=3).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationRevocationScheduled {
                era: 1,
                nominator: 2,
                candidate: 1,
                scheduled_exit: 3,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(30, collator.total_counted, "collator's total was reduced unexpectedly");

            set_reward_pot(5);
            roll_to_era_begin(3);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended"
            );

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![Event::<Test>::Rewarded { account: 1, rewards: 5 }],
                "nominator was rewarded unexpectedly"
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                20, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );
        });
}

#[test]
fn test_nominator_scheduled_for_revoke_is_rewarded_when_request_cancelled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 2, 3 and 4
            (2..=4).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(2), 1));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationRevocationScheduled {
                era: 1,
                nominator: 2,
                candidate: 1,
                scheduled_exit: 3,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(30, collator.total_counted, "collator's total was reduced unexpectedly");

            roll_to_era_begin(2);
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![Event::<Test>::Rewarded { account: 1, rewards: 5 }],
                "nominator was rewarded unexpectedly",
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                30, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );

            set_reward_pot(5);
            roll_to_era_begin(5);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended",
            );
        });
}

#[test]
fn test_nominator_scheduled_for_bond_decrease_is_rewarded_for_previous_eras_but_less_for_future() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 20), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 1, 2 and 3
            (1..=3).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 10,));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationDecreaseScheduled {
                execute_era: 3,
                nominator: 2,
                candidate: 1,
                amount_to_decrease: 10,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(40, collator.total_counted, "collator's total was reduced unexpectedly");

            set_reward_pot(5);
            roll_to_era_begin(3);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 2 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended"
            );

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was rewarded unexpectedly"
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                30, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );
        });
}

#[test]
fn test_nominator_scheduled_for_bond_decrease_is_rewarded_when_request_cancelled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 20), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 2, 3 and 4
            (2..=4).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_nominator_bond_less(Origin::signed(2), 1, 10,));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationDecreaseScheduled {
                execute_era: 3,
                nominator: 2,
                candidate: 1,
                amount_to_decrease: 10,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(40, collator.total_counted, "collator's total was reduced unexpectedly");

            roll_to_era_begin(2);
            assert_ok!(ParachainStaking::cancel_nomination_request(Origin::signed(2), 1));

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was rewarded unexpectedly",
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                40, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );

            set_reward_pot(5);
            roll_to_era_begin(5);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 2 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended",
            );
        });
}

#[test]
fn test_nominator_scheduled_for_leave_is_rewarded_for_previous_eras_but_not_for_future() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 1, 2 and 3
            (1..=3).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2),));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitScheduled {
                era: 1,
                nominator: 2,
                scheduled_exit: 3,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(30, collator.total_counted, "collator's total was reduced unexpectedly");

            set_reward_pot(5);
            roll_to_era_begin(3);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended"
            );

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![Event::<Test>::Rewarded { account: 1, rewards: 5 },],
                "nominator was rewarded unexpectedly"
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                20, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );
        });
}

#[test]
fn test_nominator_scheduled_for_leave_is_rewarded_when_request_cancelled() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .with_nominations(vec![(2, 1, 10), (2, 3, 10)])
        .build()
        .execute_with(|| {
            // preset rewards for eras 2, 3 and 4
            (2..=4).for_each(|era| set_author(era, 1, 1));

            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitScheduled {
                era: 1,
                nominator: 2,
                scheduled_exit: 3,
            }));
            let collator = ParachainStaking::candidate_info(1).expect("candidate must exist");
            assert_eq!(
                1, collator.nomination_count,
                "collator's nominator count was reduced unexpectedly"
            );
            assert_eq!(30, collator.total_counted, "collator's total was reduced unexpectedly");

            roll_to_era_begin(2);
            assert_ok!(ParachainStaking::cancel_leave_nominators(Origin::signed(2)));

            set_reward_pot(5);
            roll_to_era_begin(4);
            assert_eq_last_events!(
                vec![Event::<Test>::Rewarded { account: 1, rewards: 5 },],
                "nominator was rewarded unexpectedly",
            );
            let collator_snapshot = ParachainStaking::at_stake(ParachainStaking::era().current, 1);
            assert_eq!(
                1,
                collator_snapshot.nominations.len(),
                "collator snapshot's nominator count was reduced unexpectedly"
            );
            assert_eq!(
                30, collator_snapshot.total,
                "collator snapshot's total was reduced unexpectedly",
            );

            set_reward_pot(5);
            roll_to_era_begin(5);
            assert_eq_last_events!(
                vec![
                    Event::<Test>::Rewarded { account: 1, rewards: 3 },
                    Event::<Test>::Rewarded { account: 2, rewards: 2 },
                ],
                "nominator was not rewarded as intended",
            );
        });
}

#[test]
fn test_nomination_request_exists_returns_false_when_nothing_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert!(!ParachainStaking::nomination_request_exists(&1, &2));
        });
}

#[test]
fn test_nomination_request_exists_returns_true_when_decrease_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominationScheduledRequests<Test>>::insert(
                1,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
            assert!(ParachainStaking::nomination_request_exists(&1, &2));
        });
}

#[test]
fn test_nomination_request_exists_returns_true_when_revoke_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominationScheduledRequests<Test>>::insert(
                1,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Revoke(5),
                }],
            );
            assert!(ParachainStaking::nomination_request_exists(&1, &2));
        });
}

#[test]
fn test_nomination_request_revoke_exists_returns_false_when_nothing_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            assert!(!ParachainStaking::nomination_request_revoke_exists(&1, &2));
        });
}

#[test]
fn test_nomination_request_revoke_exists_returns_false_when_decrease_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominationScheduledRequests<Test>>::insert(
                1,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Decrease(5),
                }],
            );
            assert!(!ParachainStaking::nomination_request_revoke_exists(&1, &2));
        });
}

#[test]
fn test_nomination_request_revoke_exists_returns_true_when_revoke_exists() {
    ExtBuilder::default()
        .with_balances(vec![(1, 30), (2, 25)])
        .with_candidates(vec![(1, 30)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominationScheduledRequests<Test>>::insert(
                1,
                vec![ScheduledRequest {
                    nominator: 2,
                    when_executable: 3,
                    action: NominationAction::Revoke(5),
                }],
            );
            assert!(ParachainStaking::nomination_request_revoke_exists(&1, &2));
        });
}

#[test]
fn test_hotfix_remove_nomination_requests_exited_candidates_cleans_up() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // invalid state
            <NominationScheduledRequests<Test>>::insert(
                2,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            <NominationScheduledRequests<Test>>::insert(
                3,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            assert_ok!(ParachainStaking::hotfix_remove_nomination_requests_exited_candidates(
                Origin::signed(1),
                vec![2, 3, 4] // 4 does not exist, but is OK for idempotency
            ));

            assert!(!<NominationScheduledRequests<Test>>::contains_key(2));
            assert!(!<NominationScheduledRequests<Test>>::contains_key(3));
        });
}

#[test]
fn test_hotfix_remove_nomination_requests_exited_candidates_cleans_up_only_specified_keys() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // invalid state
            <NominationScheduledRequests<Test>>::insert(
                2,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            <NominationScheduledRequests<Test>>::insert(
                3,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            assert_ok!(ParachainStaking::hotfix_remove_nomination_requests_exited_candidates(
                Origin::signed(1),
                vec![2]
            ));

            assert!(!<NominationScheduledRequests<Test>>::contains_key(2));
            assert!(<NominationScheduledRequests<Test>>::contains_key(3));
        });
}

#[test]
fn test_hotfix_remove_nomination_requests_exited_candidates_errors_when_requests_not_empty() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // invalid state
            <NominationScheduledRequests<Test>>::insert(
                2,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            <NominationScheduledRequests<Test>>::insert(
                3,
                vec![ScheduledRequest {
                    nominator: 10,
                    when_executable: 1,
                    action: NominationAction::Revoke(10),
                }],
            );

            assert_noop!(
                ParachainStaking::hotfix_remove_nomination_requests_exited_candidates(
                    Origin::signed(1),
                    vec![2, 3]
                ),
                <Error<Test>>::CandidateNotLeaving,
            );
        });
}

#[test]
fn test_hotfix_remove_nomination_requests_exited_candidates_errors_when_candidate_not_exited() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20)])
        .with_candidates(vec![(1, 20)])
        .build()
        .execute_with(|| {
            // invalid state
            <NominationScheduledRequests<Test>>::insert(
                1,
                Vec::<ScheduledRequest<u64, u128>>::new(),
            );
            assert_noop!(
                ParachainStaking::hotfix_remove_nomination_requests_exited_candidates(
                    Origin::signed(1),
                    vec![1]
                ),
                <Error<Test>>::CandidateNotLeaving,
            );
        });
}

#[test]
fn locking_zero_amount_is_ignored() {
    use frame_support::traits::{LockableCurrency, WithdrawReasons};

    // this test demonstrates the behavior of pallet Balance's `LockableCurrency` implementation of
    // `set_locks()` when an amount of 0 is provided: it is a no-op

    ExtBuilder::default().with_balances(vec![(1, 100)]).build().execute_with(|| {
        assert_eq!(crate::mock::query_lock_amount(1, NOMINATOR_LOCK_ID), None);

        Balances::set_lock(NOMINATOR_LOCK_ID, &1, 1, WithdrawReasons::all());
        assert_eq!(crate::mock::query_lock_amount(1, NOMINATOR_LOCK_ID), Some(1));

        Balances::set_lock(NOMINATOR_LOCK_ID, &1, 0, WithdrawReasons::all());
        // Note that we tried to call `set_lock(0)` and it ignored it, we still have our lock
        assert_eq!(crate::mock::query_lock_amount(1, NOMINATOR_LOCK_ID), Some(1));
    });
}

#[test]
fn revoke_last_removes_lock() {
    ExtBuilder::default()
        .with_balances(vec![(1, 100), (2, 100), (3, 100)])
        .with_candidates(vec![(1, 25), (2, 25)])
        .with_nominations(vec![(3, 1, 30), (3, 2, 25)])
        .build()
        .execute_with(|| {
            assert_eq!(crate::mock::query_lock_amount(3, NOMINATOR_LOCK_ID), Some(55));

            // schedule and remove one...
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(3), 1));
            roll_to_era_begin(3);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(3), 3, 1));
            assert_eq!(crate::mock::query_lock_amount(3, NOMINATOR_LOCK_ID), Some(25));

            // schedule and remove the other...
            assert_ok!(ParachainStaking::schedule_revoke_nomination(Origin::signed(3), 2));
            roll_to_era_begin(5);
            assert_ok!(ParachainStaking::execute_nomination_request(Origin::signed(3), 3, 2));
            assert_eq!(crate::mock::query_lock_amount(3, NOMINATOR_LOCK_ID), None);
        });
}

#[allow(deprecated)]
#[test]
fn test_nominator_with_deprecated_status_leaving_can_schedule_leave_nominators_as_fix() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominatorState<Test>>::mutate(2, |value| {
                value.as_mut().map(|mut state| {
                    state.status = NominatorStatus::Leaving(2);
                })
            });
            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Leaving(_)));

            assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(2)));
            assert!(<NominationScheduledRequests<Test>>::get(1)
                .iter()
                .any(|r| r.nominator == 2 && matches!(r.action, NominationAction::Revoke(_))));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitScheduled {
                era: 1,
                nominator: 2,
                scheduled_exit: 3
            }));

            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Active));
        });
}

#[allow(deprecated)]
#[test]
fn test_nominator_with_deprecated_status_leaving_can_cancel_leave_nominators_as_fix() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominatorState<Test>>::mutate(2, |value| {
                value.as_mut().map(|mut state| {
                    state.status = NominatorStatus::Leaving(2);
                })
            });
            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Leaving(_)));

            assert_ok!(ParachainStaking::cancel_leave_nominators(Origin::signed(2)));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominatorExitCancelled {
                nominator: 2
            }));

            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Active));
        });
}

#[allow(deprecated)]
#[test]
fn test_nominator_with_deprecated_status_leaving_can_execute_leave_nominators_as_fix() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominatorState<Test>>::mutate(2, |value| {
                value.as_mut().map(|mut state| {
                    state.status = NominatorStatus::Leaving(2);
                })
            });
            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Leaving(_)));

            roll_to(10);
            assert_ok!(ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1));
            assert_event_emitted!(Event::NominatorLeft { nominator: 2, unstaked_amount: 10 });

            let state = <NominatorState<Test>>::get(2);
            assert!(state.is_none());
        });
}

#[allow(deprecated)]
#[test]
fn test_nominator_with_deprecated_status_leaving_cannot_execute_leave_nominators_early_no_fix() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40)])
        .with_candidates(vec![(1, 20)])
        .with_nominations(vec![(2, 1, 10)])
        .build()
        .execute_with(|| {
            <NominatorState<Test>>::mutate(2, |value| {
                value.as_mut().map(|mut state| {
                    state.status = NominatorStatus::Leaving(2);
                })
            });
            let state = <NominatorState<Test>>::get(2);
            assert!(matches!(state.unwrap().status, NominatorStatus::Leaving(_)));

            assert_noop!(
                ParachainStaking::execute_leave_nominators(Origin::signed(2), 2, 1),
                Error::<Test>::NominatorCannotLeaveYet
            );
        });
}
