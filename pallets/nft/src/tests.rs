#![cfg(test)]

use crate::{mock::*, pallet::Error, *};
use frame_support::{assert_noop, assert_ok};

// This function checks that nft ownership is set correctly in storage.
// This will panic if things are not correct.
fn assert_ownership(owner: u64, nft_id: [u8; 16]) {
	// For a nft to be owned it should exist.
	let nft = NFTs::<Test>::get(nft_id).unwrap();
	// The nft's owner is set correctly.
	assert_eq!(nft.owner, owner);

	for (check_owner, owned) in NFTsOwned::<Test>::iter() {
		if owner == check_owner {
			// Owner should have this nft.
			assert!(owned.contains(&nft_id));
		} else {
			// Everyone else should not.
			assert!(!owned.contains(&nft_id));
		}
	}
}

#[test]
fn should_build_genesis_nft() {
	new_test_ext(vec![
		(1, *b"1234567890123456"),
		(2, *b"123456789012345a"),
	])
	.execute_with(|| {
		// Check we have 2 nfts, as specified in genesis
		assert_eq!(CountForNFTs::<Test>::get(), 2);

		// Check owners own the correct amount of nfts
		let nfts_owned_by_1 = NFTsOwned::<Test>::get(1);
		assert_eq!(nfts_owned_by_1.len(), 1);

		let nfts_owned_by_2 = NFTsOwned::<Test>::get(2);
		assert_eq!(nfts_owned_by_2.len(), 1);

		// Check that nft are owned by the correct owners
		let nft_1 = nfts_owned_by_1[0];
		assert_ownership(1, nft_1);

		let nft_2 = nfts_owned_by_2[0];
		assert_ownership(2, nft_2);
	});
}

#[test]
fn create_nft_should_work() {
	new_test_ext(vec![])
	.execute_with(|| {
		// Create a nft with account #10
		assert_ok!(SubstrateNFT::create_nft(Origin::signed(10)));

		// Check that now 3 nfts exists
		assert_eq!(CountForNFTs::<Test>::get(), 1);

		// Check that account #10 owns 1 nft
		let nfts_owned = NFTsOwned::<Test>::get(10);
		assert_eq!(nfts_owned.len(), 1);
		let id = nfts_owned.last().unwrap();
		assert_ownership(10, *id);

		// Check that multiple create_nft calls work in the same block.
		// Increment extrinsic index to add entropy for DNA
		frame_system::Pallet::<Test>::set_extrinsic_index(1);
		assert_ok!(SubstrateNFT::create_nft(Origin::signed(10)));
	});
}

#[test]
fn create_nft_fails() {
	// Check that create_nft fails when user owns too many nfts.
	new_test_ext(vec![])
	.execute_with(|| {
		// Create `MaxNFTsOwned` nfts with account #10
		for _i in 0..<Test as Config>::MaxNFTsOwned::get() {
			assert_ok!(SubstrateNFT::create_nft(Origin::signed(10)));
			// We do this because the hash of the nft depends on this for seed,
			// so changing this allows you to have a different nft id
			System::set_block_number(System::block_number() + 1);
		}

		// Can't create 1 more
		assert_noop!(
			SubstrateNFT::create_nft(Origin::signed(10)),
			Error::<Test>::TooManyOwned
		);

		// Minting a nft with DNA that already exists should fail
		let id = [0u8; 16];

		// Mint new nft with `id`
		assert_ok!(SubstrateNFT::mint(&1, id));

		// Mint another nft with the same `id` should fail
		assert_noop!(SubstrateNFT::mint(&1, id), Error::<Test>::DuplicateNFT);
	});
}

#[test]
fn bid_nft_should_work() {
	new_test_ext(vec![
		(1, *b"1234567890123456"),
	])
	.execute_with(|| {
		// Bid a nft with account #10
		assert_ok!(SubstrateNFT::bid(Origin::signed(10), *b"1234567890123456", 10));

		let nft = NFTs::<Test>::get(*b"1234567890123456").unwrap();

		assert_eq!(10, nft.bidder.unwrap());
	});
}

#[test]
fn bid_nft_fail() {
	new_test_ext(vec![
		(1, *b"1234567890123456"),
	])
	.execute_with(|| {
		// Bid a nft with account #1
		assert_noop!(SubstrateNFT::bid(Origin::signed(1), *b"1234567890123456", 10), 
			Error::<Test>::SameBidderOwner);
	});
}

#[test]
fn withdrawal_nft_should_work() {
	new_test_ext(vec![
		(1, *b"1234567890123456"),
		(2, *b"123456789012345a"),
	])
	.execute_with(|| {
		Balances::free_balance(&1);
		Balances::free_balance(&2);

		assert_ok!(SubstrateNFT::bid(Origin::signed(2), *b"1234567890123456", 1));

		System::set_block_number(System::block_number() + 200);

		assert_ok!(SubstrateNFT::withdrawal(Origin::signed(1), *b"1234567890123456"));
	});
}

#[test]
fn withdrawal_nft_fail() {
	new_test_ext(vec![
		(1, *b"1234567890123456"),
		(2, *b"123456789012345a"),
	])
	.execute_with(|| {
		Balances::free_balance(&1);
		Balances::free_balance(&2);

		assert_ok!(SubstrateNFT::bid(Origin::signed(2), *b"1234567890123456", 5));

		assert_noop!(SubstrateNFT::withdrawal(Origin::signed(1), *b"1234567890123456"), 
			Error::<Test>::BidNotClosed);
	});
}
