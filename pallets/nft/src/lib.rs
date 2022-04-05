#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
pub use pallet::*;

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use frame_support::{
        pallet_prelude::*,
        traits::{tokens::ExistenceRequirement, Currency, Randomness},
    };
    use sp_io::hashing::blake2_128;
    use scale_info::TypeInfo;
    use sp_runtime::ArithmeticError;

    #[cfg(feature = "std")]
    use frame_support::serde::{Deserialize, Serialize};

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // Struct for holding NFT information.
    #[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    #[codec(mel_bound())]
    pub struct NFT<T: Config> {
        pub dna: [u8; 16],
        pub bid_price: Option<BalanceOf<T>>,
        pub bidder: Option<T::AccountId>,
        pub owner: T::AccountId,
        pub bn: T::BlockNumber,
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The Currency handler for the nft pallet.
        type Currency: Currency<Self::AccountId>;

        type NFTRandomness: Randomness<Self::Hash, Self::BlockNumber>;

        #[pallet::constant]
        type MaxNFTsOwned: Get<u32>;
	}

	// The pallet's runtime storage items.
	#[pallet::storage]
    #[pallet::getter(fn nft_cnt)]
    pub(super) type CountForNFTs<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
    #[pallet::getter(fn nfts)]
    pub(super) type NFTs<T: Config> = StorageMap<
        _,
        Twox64Concat,
        [u8; 16],
        NFT<T>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn nfts_owned)]
    /// Keeps track of what accounts own what Kitty.
    pub(super) type NFTsOwned<T: Config> = StorageMap<
        _,
        Twox64Concat,
        T::AccountId,
        BoundedVec<[u8; 16], T::MaxNFTsOwned>,
        ValueQuery,
    >;

	// Pallets use events to inform users when important changes are made.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new nft was successfully created.
        Created { nft: [u8; 16], owner: T::AccountId },
        /// A nft was successfully bidded.
        Bidded { nft: [u8; 16], bidder: T::AccountId, bid_price: BalanceOf<T> },
        /// A nft was successfully withdrawal.
        Withdrawal { nft: [u8; 16], from: T::AccountId, to: T::AccountId },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// An account may only own `MaxNFTOwned` nfts.
        TooManyOwned,
		/// This nft already exists!
        DuplicateNFT,
        /// This nft does not exist!
        NoNFT,
        /// Bidder is same as owner
        SameBidderOwner,
        /// Ensures that the bid price is greater than the previous price.
        BidPriceTooLow,
        /// Bid has been closed.
        BidClosed,
        /// Bid has been not closed.
        BidNotClosed,
        /// Withdrawal's nft error owner
        WithdrawalNotOwner,
        /// No bidder
        NoBidder,
	}

    // Our pallet's genesis configuration
    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub nfts: Vec<(T::AccountId, [u8; 16])>,
    }

    // Required to implement default for GenesisConfig
    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> GenesisConfig<T> {
            GenesisConfig { nfts: vec![] }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            // When building a kitty from genesis config, we require the DNA and Gender to be
            // supplied
            for (account, dna) in &self.nfts {
                assert!(Pallet::<T>::mint(account, *dna).is_ok());
            }
        }
    }

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new unique nft.
        ///
        /// The actual nft creation is done in the `mint()` function.
        #[pallet::weight(0)]
        pub fn create_nft(origin: OriginFor<T>) -> DispatchResult {
            // Make sure the caller is from a signed origin
            let sender = ensure_signed(origin)?;

            // Generate unique DNA using a helper function
            let nft_gen_dna = Self::gen_dna();

            // Write new nft to storage by calling helper function
            Self::mint(&sender, nft_gen_dna)?;

            Ok(())
        }

        /// Bid a new unique nft.
        #[pallet::weight(0)]
        pub fn bid(origin: OriginFor<T>, 
        		   nft_id: [u8; 16], 
        		   price: BalanceOf<T>) -> DispatchResult {
        	// Make sure the caller is from a signed origin
            let sender = ensure_signed(origin)?;

            let mut nft = NFTs::<T>::get(&nft_id).ok_or(Error::<T>::NoNFT)?;

            ensure!(sender != nft.owner, Error::<T>::SameBidderOwner);

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            ensure!(current_block_number < nft.bn, Error::<T>::BidClosed);

            if let Some(bid_price) = nft.bid_price {
            	ensure!(price > bid_price, Error::<T>::BidPriceTooLow);
            }

            nft.bid_price = Some(price);
            nft.bidder = Some(sender.clone());

            NFTs::<T>::insert(&nft_id, nft);

            Self::deposit_event(Event::Bidded { nft: nft_id, bidder: sender.clone(), bid_price: price });

        	Ok(())
        }

        /// Withdrawal for one nft
        #[pallet::weight(0)]
        pub fn withdrawal(origin: OriginFor<T>, nft_id: [u8; 16]) -> DispatchResult {
        	// Make sure the caller is from a signed origin
            let sender = ensure_signed(origin)?;

            // Get the nft
            let mut nft = NFTs::<T>::get(&nft_id).ok_or(Error::<T>::NoNFT)?;
            let from = nft.owner;

            ensure!(from == sender, Error::<T>::WithdrawalNotOwner);

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            ensure!(current_block_number >= nft.bn, Error::<T>::BidNotClosed);

            let mut from_owned = NFTsOwned::<T>::get(&from);

            // Remove nft from list of owned nfts.
            if let Some(ind) = from_owned.iter().position(|&id| id == nft_id) {
                from_owned.swap_remove(ind);
            } else {
                return Err(Error::<T>::NoNFT.into());
            }

            // Add nft to the list of owned nfts.
            if let Some(to) = nft.bidder {
                if let Some(bid_price) = nft.bid_price {
    	            let mut to_owned = NFTsOwned::<T>::get(&to);
    	            to_owned.try_push(nft_id).map_err(|()| Error::<T>::TooManyOwned)?;

    	            T::Currency::transfer(&to, &from, bid_price, ExistenceRequirement::KeepAlive)?;

    	            Self::deposit_event(Event::Withdrawal { nft: nft_id, from: from.clone(), to: to.clone() });
                } else {
                    return Err(Error::<T>::NoBidder.into());
                }
        	} else {
        		return Err(Error::<T>::NoBidder.into());
        	}

        	Ok(())
        }
	}

	impl<T: Config> Pallet<T> {
		// Generates and returns DNA
        pub fn gen_dna() -> [u8; 16] {
            // Create randomness
            let random = T::NFTRandomness::random(&b"dna"[..]).0;

            // Create randomness payload. Multiple kitties can be generated in the same block,
            // retaining uniqueness.
            let unique_payload = (
                random,
                frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default(),
                frame_system::Pallet::<T>::block_number(),
            );

            // Turns into a byte array
            let encoded_payload = unique_payload.encode();
            let hash = blake2_128(&encoded_payload);

            hash
        }

        // Helper to mint a kitty
        pub fn mint(
            owner: &T::AccountId,
            dna: [u8; 16],
        ) -> Result<[u8; 16], DispatchError> {
            // Create a new object
            let bn = Self::get_block_number();
            let nft = NFT::<T> { dna, bid_price: None, bidder: None, owner: owner.clone(), bn };

            // Check if the kitty does not already exist in our storage map
            ensure!(!NFTs::<T>::contains_key(&nft.dna), Error::<T>::DuplicateNFT);

            // Performs this operation first as it may fail
            let count = CountForNFTs::<T>::get();
            let new_count = count.checked_add(1).ok_or(ArithmeticError::Overflow)?;

            // Append kitty to KittiesOwned
            NFTsOwned::<T>::try_append(&owner, nft.dna)
                .map_err(|_| Error::<T>::TooManyOwned)?;

            // Write new kitty to storage
            NFTs::<T>::insert(nft.dna, nft);
            CountForNFTs::<T>::put(new_count);

            // Deposit our "Created" event.
            Self::deposit_event(Event::Created { nft: dna, owner: owner.clone() });

            // Returns the DNA of the new kitty if this succeeds
            Ok(dna)
        }

        pub fn get_block_number() -> T::BlockNumber {
            let current_block_number = <frame_system::Pallet<T>>::block_number();
            let block_delay: u32 = 100;
            let bn = current_block_number + block_delay.into();
            bn
        }
	}
}
