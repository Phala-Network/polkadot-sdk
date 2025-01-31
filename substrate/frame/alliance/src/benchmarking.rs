// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Alliance pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use core::{
	cmp,
	convert::{TryFrom, TryInto},
	mem::size_of,
};
use sp_runtime::traits::{Bounded, Hash, StaticLookup};

use frame_benchmarking::{account, impl_benchmark_test_suite, v2::*, BenchmarkError};
use frame_support::traits::{EnsureOrigin, Get, UnfilteredDispatchable};
use frame_system::{pallet_prelude::BlockNumberFor, Pallet as System, RawOrigin as SystemOrigin};

use super::{Call as AllianceCall, Pallet as Alliance, *};

const SEED: u32 = 0;

const MAX_BYTES: u32 = 1_024;

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn cid(input: impl AsRef<[u8]>) -> Cid {
	let result = sp_crypto_hashing::sha2_256(input.as_ref());
	Cid::new_v0(result)
}

fn rule(input: impl AsRef<[u8]>) -> Cid {
	cid(input)
}

fn announcement(input: impl AsRef<[u8]>) -> Cid {
	cid(input)
}

fn funded_account<T: Config<I>, I: 'static>(name: &'static str, index: u32) -> T::AccountId {
	let account: T::AccountId = account(name, index, SEED);
	T::Currency::make_free_balance_be(&account, BalanceOf::<T, I>::max_value() / 100u8.into());
	account
}

fn fellow<T: Config<I>, I: 'static>(index: u32) -> T::AccountId {
	funded_account::<T, I>("fellow", index)
}

fn ally<T: Config<I>, I: 'static>(index: u32) -> T::AccountId {
	funded_account::<T, I>("ally", index)
}

fn outsider<T: Config<I>, I: 'static>(index: u32) -> T::AccountId {
	funded_account::<T, I>("outsider", index)
}

fn generate_unscrupulous_account<T: Config<I>, I: 'static>(index: u32) -> T::AccountId {
	funded_account::<T, I>("unscrupulous", index)
}

fn set_members<T: Config<I>, I: 'static>() {
	let fellows: BoundedVec<_, T::MaxMembersCount> =
		BoundedVec::try_from(vec![fellow::<T, I>(1), fellow::<T, I>(2)]).unwrap();
	fellows.iter().for_each(|who| {
		T::Currency::reserve(&who, T::AllyDeposit::get()).unwrap();
		<DepositOf<T, I>>::insert(&who, T::AllyDeposit::get());
	});
	Members::<T, I>::insert(MemberRole::Fellow, fellows.clone());

	let allies: BoundedVec<_, T::MaxMembersCount> =
		BoundedVec::try_from(vec![ally::<T, I>(1)]).unwrap();
	allies.iter().for_each(|who| {
		T::Currency::reserve(&who, T::AllyDeposit::get()).unwrap();
		<DepositOf<T, I>>::insert(&who, T::AllyDeposit::get());
	});
	Members::<T, I>::insert(MemberRole::Ally, allies);

	T::InitializeMembers::initialize_members(&[fellows.as_slice()].concat());
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	// This tests when proposal is created and queued as "proposed"
	#[benchmark]
	fn propose_proposed(
		b: Linear<1, MAX_BYTES>,
		m: Linear<2, { T::MaxFellows::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();
		let proposer = fellows[0].clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		let threshold = m;
		// Add previous proposals.
		for i in 0..p - 1 {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; b as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal),
				bytes_in_storage,
			)?;
		}

		let proposal: T::Proposal =
			AllianceCall::<T, I>::set_rule { rule: rule(vec![p as u8; b as usize]) }.into();

		#[extrinsic_call]
		propose(
			SystemOrigin::Signed(proposer.clone()),
			threshold,
			Box::new(proposal.clone()),
			bytes_in_storage,
		);

		let proposal_hash = T::Hashing::hash_of(&proposal);
		assert_eq!(T::ProposalProvider::proposal_of(proposal_hash), Some(proposal));
		Ok(())
	}

	#[benchmark]
	fn vote(m: Linear<5, { T::MaxFellows::get() }>) -> Result<(), BenchmarkError> {
		let p = T::MaxProposals::get();
		let b = MAX_BYTES;
		let _bytes_in_storage = b + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();
		let proposer = fellows[0].clone();

		let members = fellows.clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		// Threshold is 1 less than the number of members so that one person can vote nay
		let threshold = m - 1;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; b as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				b,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
		}

		let index = p - 1;
		// Have almost everyone vote aye on last proposal, while keeping it from passing.
		for j in 0..m - 3 {
			let voter = &members[j as usize];
			Alliance::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				true,
			)?;
		}

		let voter = members[m as usize - 3].clone();
		// Voter votes aye without resolving the vote.
		Alliance::<T, I>::vote(SystemOrigin::Signed(voter.clone()).into(), last_hash, index, true)?;

		// Voter switches vote to nay, but does not kill the vote, just updates + inserts
		let approve = false;

		// Whitelist voter account from further DB operations.
		let voter_key = frame_system::Account::<T>::hashed_key_for(&voter);
		frame_benchmarking::benchmarking::add_to_whitelist(voter_key.into());

		#[extrinsic_call]
		_(SystemOrigin::Signed(voter), last_hash, index, approve);

		//nothing to verify
		Ok(())
	}

	#[benchmark]
	fn close_early_disapproved(
		m: Linear<4, { T::MaxFellows::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes = 100;
		let bytes_in_storage = bytes + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();

		let members = fellows.clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		let proposer = members[0].clone();
		let voter = members[1].clone();

		// Threshold is total members so that one nay will disapprove the vote
		let threshold = m;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; bytes as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
			assert_eq!(T::ProposalProvider::proposal_of(last_hash), Some(proposal));
		}

		let index = p - 1;
		// Have most everyone vote aye on last proposal, while keeping it from passing.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			Alliance::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				true,
			)?;
		}

		// Voter votes aye without resolving the vote.
		Alliance::<T, I>::vote(SystemOrigin::Signed(voter.clone()).into(), last_hash, index, true)?;

		// Voter switches vote to nay, which kills the vote
		Alliance::<T, I>::vote(
			SystemOrigin::Signed(voter.clone()).into(),
			last_hash,
			index,
			false,
		)?;

		// Whitelist voter account from further DB operations.
		let voter_key = frame_system::Account::<T>::hashed_key_for(&voter);
		frame_benchmarking::benchmarking::add_to_whitelist(voter_key.into());

		#[extrinsic_call]
		close(SystemOrigin::Signed(voter), last_hash, index, Weight::MAX, bytes_in_storage);

		assert_eq!(T::ProposalProvider::proposal_of(last_hash), None);
		Ok(())
	}

	// We choose 4 as a minimum for param m, so we always trigger a vote in the voting loop (`for j
	// in ...`)
	#[benchmark]
	fn close_early_approved(
		b: Linear<1, MAX_BYTES>,
		m: Linear<4, { T::MaxFellows::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();

		let members = fellows.clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		let proposer = members[0].clone();
		// Threshold is 2 so any two ayes will approve the vote
		let threshold = 2;

		// Add previous proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; b as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
			assert_eq!(T::ProposalProvider::proposal_of(last_hash), Some(proposal));
		}

		let index = p - 1;
		// Caller switches vote to nay on their own proposal, allowing them to be the deciding
		// approval vote
		Alliance::<T, I>::vote(
			SystemOrigin::Signed(proposer.clone()).into(),
			last_hash,
			index,
			false,
		)?;

		// Have almost everyone vote nay on last proposal, while keeping it from failing.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			Alliance::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				false,
			)?;
		}

		// Member zero is the first aye
		Alliance::<T, I>::vote(
			SystemOrigin::Signed(members[0].clone()).into(),
			last_hash,
			index,
			true,
		)?;

		let voter = members[1].clone();
		// Caller switches vote to aye, which passes the vote
		Alliance::<T, I>::vote(SystemOrigin::Signed(voter.clone()).into(), last_hash, index, true)?;

		#[extrinsic_call]
		close(SystemOrigin::Signed(voter), last_hash, index, Weight::MAX, bytes_in_storage);

		assert_eq!(T::ProposalProvider::proposal_of(last_hash), None);
		Ok(())
	}

	#[benchmark]
	fn close_disapproved(
		m: Linear<2, { T::MaxFellows::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes = 100;
		let bytes_in_storage = bytes + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();

		let members = fellows.clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		let proposer = members[0].clone();
		let voter = members[1].clone();

		// Threshold is one less than total members so that two nays will disapprove the vote
		let threshold = m - 1;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; bytes as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
			assert_eq!(T::ProposalProvider::proposal_of(last_hash), Some(proposal));
		}

		let index = p - 1;
		// Have almost everyone vote aye on last proposal, while keeping it from passing.
		// A few abstainers will be the nay votes needed to fail the vote.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			Alliance::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				true,
			)?;
		}

		Alliance::<T, I>::vote(
			SystemOrigin::Signed(voter.clone()).into(),
			last_hash,
			index,
			false,
		)?;

		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());

		#[extrinsic_call]
		close(SystemOrigin::Signed(voter), last_hash, index, Weight::MAX, bytes_in_storage);

		// The last proposal is removed.
		assert_eq!(T::ProposalProvider::proposal_of(last_hash), None);
		Ok(())
	}

	// We choose 5 fellows as a minimum so we always trigger a vote in the voting loop (`for j in
	// ...`)
	#[benchmark]
	fn close_approved(
		b: Linear<1, MAX_BYTES>,
		m: Linear<5, { T::MaxFellows::get() }>,
		p: Linear<1, { T::MaxProposals::get() }>,
	) -> Result<(), BenchmarkError> {
		let bytes_in_storage = b + size_of::<Cid>() as u32 + 32;

		// Construct `members`.
		let fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();

		let members = fellows.clone();

		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows, vec![])?;

		let proposer = members[0].clone();
		// Threshold is two, so any two ayes will pass the vote
		let threshold = 2;

		// Add proposals
		let mut last_hash = T::Hash::default();
		for i in 0..p {
			// Proposals should be different so that different proposal hashes are generated
			let proposal: T::Proposal =
				AllianceCall::<T, I>::set_rule { rule: rule(vec![i as u8; b as usize]) }.into();
			Alliance::<T, I>::propose(
				SystemOrigin::Signed(proposer.clone()).into(),
				threshold,
				Box::new(proposal.clone()),
				bytes_in_storage,
			)?;
			last_hash = T::Hashing::hash_of(&proposal);
			assert_eq!(T::ProposalProvider::proposal_of(last_hash), Some(proposal));
		}

		// The prime member votes aye, so abstentions default to aye.
		Alliance::<T, I>::vote(
			SystemOrigin::Signed(proposer.clone()).into(),
			last_hash,
			p - 1,
			true, // Vote aye.
		)?;

		let index = p - 1;
		// Have almost everyone vote nay on last proposal, while keeping it from failing.
		// A few abstainers will be the aye votes needed to pass the vote.
		for j in 2..m - 1 {
			let voter = &members[j as usize];
			Alliance::<T, I>::vote(
				SystemOrigin::Signed(voter.clone()).into(),
				last_hash,
				index,
				false,
			)?;
		}

		// caller is prime, prime already votes aye by creating the proposal
		System::<T>::set_block_number(BlockNumberFor::<T>::max_value());

		#[extrinsic_call]
		close(SystemOrigin::Signed(proposer), last_hash, index, Weight::MAX, bytes_in_storage);

		assert_eq!(T::ProposalProvider::proposal_of(last_hash), None);
		Ok(())
	}

	#[benchmark]
	fn init_members(
		m: Linear<1, { T::MaxFellows::get() }>,
		z: Linear<0, { T::MaxAllies::get() }>,
	) -> Result<(), BenchmarkError> {
		let mut fellows = (0..m).map(fellow::<T, I>).collect::<Vec<_>>();
		let mut allies = (0..z).map(ally::<T, I>).collect::<Vec<_>>();

		#[extrinsic_call]
		_(SystemOrigin::Root, fellows.clone(), allies.clone());

		fellows.sort();
		allies.sort();
		assert_last_event::<T, I>(
			Event::MembersInitialized { fellows: fellows.clone(), allies: allies.clone() }.into(),
		);
		assert_eq!(Alliance::<T, I>::members(MemberRole::Fellow), fellows);
		assert_eq!(Alliance::<T, I>::members(MemberRole::Ally), allies);
		Ok(())
	}

	#[benchmark]
	fn disband(
		x: Linear<1, { T::MaxFellows::get() }>,
		y: Linear<0, { T::MaxAllies::get() }>,
		z: Linear<0, { T::MaxMembersCount::get() / 2 }>,
	) -> Result<(), BenchmarkError> {
		let fellows = (0..x).map(fellow::<T, I>).collect::<Vec<_>>();
		let allies = (0..y).map(ally::<T, I>).collect::<Vec<_>>();
		let witness = DisbandWitness { fellow_members: x, ally_members: y };

		// setting the Alliance to disband on the benchmark call
		Alliance::<T, I>::init_members(SystemOrigin::Root.into(), fellows.clone(), allies.clone())?;

		// reserve deposits
		let deposit = T::AllyDeposit::get();
		for member in fellows.iter().chain(allies.iter()).take(z as usize) {
			T::Currency::reserve(&member, deposit)?;
			<DepositOf<T, I>>::insert(&member, deposit);
		}

		assert_eq!(Alliance::<T, I>::voting_members_count(), x);
		assert_eq!(Alliance::<T, I>::ally_members_count(), y);

		#[extrinsic_call]
		_(SystemOrigin::Root, witness);

		assert_last_event::<T, I>(
			Event::AllianceDisbanded {
				fellow_members: x,
				ally_members: y,
				unreserved: cmp::min(z, x + y),
			}
			.into(),
		);

		assert!(!Alliance::<T, I>::is_initialized());
		Ok(())
	}

	#[benchmark]
	fn set_rule() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let rule = rule(b"hello world");

		let call = Call::<T, I>::set_rule { rule: rule.clone() };
		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}
		assert_eq!(Alliance::<T, I>::rule(), Some(rule.clone()));
		assert_last_event::<T, I>(Event::NewRuleSet { rule }.into());
		Ok(())
	}

	#[benchmark]
	fn announce() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let announcement = announcement(b"hello world");

		let call = Call::<T, I>::announce { announcement: announcement.clone() };
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert!(Alliance::<T, I>::announcements().contains(&announcement));
		assert_last_event::<T, I>(Event::Announced { announcement }.into());
		Ok(())
	}

	#[benchmark]
	fn remove_announcement() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let announcement = announcement(b"hello world");
		let announcements: BoundedVec<_, T::MaxAnnouncementsCount> =
			BoundedVec::try_from(vec![announcement.clone()]).unwrap();
		Announcements::<T, I>::put(announcements);

		let call = Call::<T, I>::remove_announcement { announcement: announcement.clone() };
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert!(!Alliance::<T, I>::announcements().contains(&announcement));
		assert_last_event::<T, I>(Event::AnnouncementRemoved { announcement }.into());
		Ok(())
	}

	#[benchmark]
	fn join_alliance() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let outsider = outsider::<T, I>(1);
		assert!(!Alliance::<T, I>::is_member(&outsider));
		assert_eq!(DepositOf::<T, I>::get(&outsider), None);

		#[extrinsic_call]
		_(SystemOrigin::Signed(outsider.clone()));

		assert!(Alliance::<T, I>::is_member_of(&outsider, MemberRole::Ally)); // outsider is now an ally
		assert_eq!(DepositOf::<T, I>::get(&outsider), Some(T::AllyDeposit::get())); // with a deposit
		assert!(!Alliance::<T, I>::has_voting_rights(&outsider)); // allies don't have voting rights
		assert_last_event::<T, I>(
			Event::NewAllyJoined {
				ally: outsider,
				nominator: None,
				reserved: Some(T::AllyDeposit::get()),
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn nominate_ally() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let fellow1 = fellow::<T, I>(1);
		assert!(Alliance::<T, I>::is_member_of(&fellow1, MemberRole::Fellow));

		let outsider = outsider::<T, I>(1);
		assert!(!Alliance::<T, I>::is_member(&outsider));
		assert_eq!(DepositOf::<T, I>::get(&outsider), None);

		let outsider_lookup = T::Lookup::unlookup(outsider.clone());

		#[extrinsic_call]
		_(SystemOrigin::Signed(fellow1.clone()), outsider_lookup);

		assert!(Alliance::<T, I>::is_member_of(&outsider, MemberRole::Ally)); // outsider is now an ally
		assert_eq!(DepositOf::<T, I>::get(&outsider), None); // without a deposit
		assert!(!Alliance::<T, I>::has_voting_rights(&outsider)); // allies don't have voting rights
		assert_last_event::<T, I>(
			Event::NewAllyJoined { ally: outsider, nominator: Some(fellow1), reserved: None }
				.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn elevate_ally() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let ally1 = ally::<T, I>(1);
		assert!(Alliance::<T, I>::is_ally(&ally1));

		let ally1_lookup = T::Lookup::unlookup(ally1.clone());
		let call = Call::<T, I>::elevate_ally { ally: ally1_lookup };
		let origin = T::MembershipManager::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert!(!Alliance::<T, I>::is_ally(&ally1));
		assert!(Alliance::<T, I>::has_voting_rights(&ally1));
		assert_last_event::<T, I>(Event::AllyElevated { ally: ally1 }.into());
		Ok(())
	}

	#[benchmark]
	fn give_retirement_notice() -> Result<(), BenchmarkError> {
		set_members::<T, I>();
		let fellow2 = fellow::<T, I>(2);

		assert!(Alliance::<T, I>::has_voting_rights(&fellow2));

		#[extrinsic_call]
		_(SystemOrigin::Signed(fellow2.clone()));

		assert!(Alliance::<T, I>::is_member_of(&fellow2, MemberRole::Retiring));

		assert_eq!(
			RetiringMembers::<T, I>::get(&fellow2),
			Some(System::<T>::block_number() + T::RetirementPeriod::get())
		);
		assert_last_event::<T, I>(Event::MemberRetirementPeriodStarted { member: fellow2 }.into());
		Ok(())
	}

	#[benchmark]
	fn retire() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let fellow2 = fellow::<T, I>(2);
		assert!(Alliance::<T, I>::has_voting_rights(&fellow2));

		assert_eq!(
			Alliance::<T, I>::give_retirement_notice(SystemOrigin::Signed(fellow2.clone()).into()),
			Ok(())
		);
		System::<T>::set_block_number(System::<T>::block_number() + T::RetirementPeriod::get());

		assert_eq!(DepositOf::<T, I>::get(&fellow2), Some(T::AllyDeposit::get()));

		#[extrinsic_call]
		_(SystemOrigin::Signed(fellow2.clone()));

		assert!(!Alliance::<T, I>::is_member(&fellow2));
		assert_eq!(DepositOf::<T, I>::get(&fellow2), None);
		assert_last_event::<T, I>(
			Event::MemberRetired { member: fellow2, unreserved: Some(T::AllyDeposit::get()) }
				.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn kick_member() -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let fellow2 = fellow::<T, I>(2);
		assert!(Alliance::<T, I>::is_member_of(&fellow2, MemberRole::Fellow));
		assert_eq!(DepositOf::<T, I>::get(&fellow2), Some(T::AllyDeposit::get()));

		let fellow2_lookup = T::Lookup::unlookup(fellow2.clone());
		let call = Call::<T, I>::kick_member { who: fellow2_lookup };
		let origin = T::MembershipManager::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert!(!Alliance::<T, I>::is_member(&fellow2));
		assert_eq!(DepositOf::<T, I>::get(&fellow2), None);
		assert_last_event::<T, I>(
			Event::MemberKicked { member: fellow2, slashed: Some(T::AllyDeposit::get()) }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn add_unscrupulous_items(
		n: Linear<0, { T::MaxUnscrupulousItems::get() }>,
		l: Linear<0, { T::MaxWebsiteUrlLength::get() }>,
	) -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let accounts = (0..n).map(|i| generate_unscrupulous_account::<T, I>(i)).collect::<Vec<_>>();
		let websites = (0..n)
			.map(|i| -> BoundedVec<u8, T::MaxWebsiteUrlLength> {
				BoundedVec::try_from(vec![i as u8; l as usize]).unwrap()
			})
			.collect::<Vec<_>>();

		let mut unscrupulous_list = Vec::with_capacity(accounts.len() + websites.len());
		unscrupulous_list.extend(accounts.into_iter().map(UnscrupulousItem::AccountId));
		unscrupulous_list.extend(websites.into_iter().map(UnscrupulousItem::Website));

		let call = Call::<T, I>::add_unscrupulous_items { items: unscrupulous_list.clone() };
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert_last_event::<T, I>(Event::UnscrupulousItemAdded { items: unscrupulous_list }.into());
		Ok(())
	}

	#[benchmark]
	fn remove_unscrupulous_items(
		n: Linear<0, { T::MaxUnscrupulousItems::get() }>,
		l: Linear<0, { T::MaxWebsiteUrlLength::get() }>,
	) -> Result<(), BenchmarkError> {
		set_members::<T, I>();

		let mut accounts =
			(0..n).map(|i| generate_unscrupulous_account::<T, I>(i)).collect::<Vec<_>>();
		accounts.sort();
		let accounts: BoundedVec<_, T::MaxUnscrupulousItems> = accounts.try_into().unwrap();
		UnscrupulousAccounts::<T, I>::put(accounts.clone());

		let mut websites = (0..n)
			.map(|i| -> BoundedVec<_, T::MaxWebsiteUrlLength> {
				BoundedVec::try_from(vec![i as u8; l as usize]).unwrap()
			})
			.collect::<Vec<_>>();
		websites.sort();
		let websites: BoundedVec<_, T::MaxUnscrupulousItems> = websites.try_into().unwrap();
		UnscrupulousWebsites::<T, I>::put(websites.clone());

		let mut unscrupulous_list = Vec::with_capacity(accounts.len() + websites.len());
		unscrupulous_list.extend(accounts.into_iter().map(UnscrupulousItem::AccountId));
		unscrupulous_list.extend(websites.into_iter().map(UnscrupulousItem::Website));

		let call = Call::<T, I>::remove_unscrupulous_items { items: unscrupulous_list.clone() };
		let origin = T::AnnouncementOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			call.dispatch_bypass_filter(origin)?;
		}

		assert_last_event::<T, I>(
			Event::UnscrupulousItemRemoved { items: unscrupulous_list }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn abdicate_fellow_status() -> Result<(), BenchmarkError> {
		set_members::<T, I>();
		let fellow2 = fellow::<T, I>(2);
		assert!(Alliance::<T, I>::has_voting_rights(&fellow2));

		#[extrinsic_call]
		_(SystemOrigin::Signed(fellow2.clone()));

		assert_last_event::<T, I>(Event::FellowAbdicated { fellow: fellow2 }.into());
		Ok(())
	}

	impl_benchmark_test_suite!(Alliance, crate::mock::new_bench_ext(), crate::mock::Test);
}
