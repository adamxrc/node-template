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
        traits::{tokens::ExistenceRequirement, Currency, Randomness, 
            LockIdentifier, LockableCurrency, WithdrawReasons, OnFinalize, OnInitialize, OffchainWorker,},
    };
    use sp_io::hashing::blake2_128;
    use scale_info::TypeInfo;
    use sp_runtime::ArithmeticError;

    #[cfg(feature = "std")]
    use frame_support::serde::{Deserialize, Serialize};

    const LOCK_ID: LockIdentifier = *b"nft_auct";

    type BalanceOf<T> =
        <<T as Config>::StakeCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // Struct for holding NFT information.
    #[derive(Copy, Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    #[codec(mel_bound())]
    pub struct NFT<T: Config> {
        pub dna: [u8; 16],  //nft的标识符
        pub bid_price: Option<BalanceOf<T>>,  //最大的竞标价格
        pub bidder: Option<T::AccountId>,  //最大价格的竞标人
        pub owner: T::AccountId,  //NFT的拥有人
        pub end: T::BlockNumber,  //基于区块号的NFT竞标结束时间
        pub settle: bool,  //是否已结算
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        //竞标成功时，需要质押竞标价格相对应的代币
        type StakeCurrency: LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>;

        //产生随机数类型，可以生成随机的nft的标识符
        type NFTRandomness: Randomness<Self::Hash, Self::BlockNumber>;

        //每个账号最大拥有的nft数量
        #[pallet::constant]
        type MaxNFTsOwned: Get<u32>;
	}

    //一共存在的nft数量
	// The pallet's runtime storage items.
	#[pallet::storage]
    #[pallet::getter(fn nft_cnt)]
    pub(super) type CountForNFTs<T: Config> = StorageValue<_, u64, ValueQuery>;

    //根据nft的标识符，存放对应的nft对象
	#[pallet::storage]
    #[pallet::getter(fn nfts)]
    pub(super) type NFTs<T: Config> = StorageMap<
        _,
        Twox64Concat,
        [u8; 16],
        NFT<T>,
    >;

    //存放每个账户所拥有的nft
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
        /// 成功创建nft事件
        Created { nft: [u8; 16], owner: T::AccountId },
        
        /// A nft was successfully bidded.
        /// 成功竞拍nft事件
        Bidded { nft: [u8; 16], bidder: T::AccountId, bid_price: BalanceOf<T> },
        
        /// A nft was successfully withdrawal.
        /// 成功结束竞拍提现nft事件
        Withdrawal { nft: [u8; 16], from: T::AccountId, to: T::AccountId },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// An account may only own `MaxNFTOwned` nfts.
        /// 超过账号最大能够拥有的nft数量
        TooManyOwned,

		/// This nft already exists!
        /// nft已存在
        DuplicateNFT,

        /// This nft does not exist!
        /// nft不存在
        NoNFT,

        /// Bidder is same as owner
        /// nft的拥有者不能参与竞拍
        SameBidderOwner,

        /// Ensures that the bid price is greater than the previous price.
        /// 竞拍nft的出价低于之前的竞拍人出价
        BidPriceTooLow,

        /// Bid has been closed.
        /// 竞拍已经结束了
        BidClosed,

        /// Bid has been not closed.
        /// 竞拍还没有结束
        BidNotClosed,

        /// Withdrawal's nft error owner
        /// 竞拍结算人不是nft的拥有人
        WithdrawalNotOwner,

        /// No bidder
        /// nft没有竞拍人参与竞拍
        NoBidder,

        /// NFT has been settled
        /// nft竞拍结束，已经结算过了
        NFTSettled,
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

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_finalize(block_number: T::BlockNumber) {
            //在每块结束前，进行成功完成竞标的自动结算
            //把竞标人的竞标价格的代币转移到NFT所有人上
            let aids = NFTsOwned::<T>::iter_keys();
            for aid in aids {
                let ids = NFTsOwned::<T>::get(&aid);
                for id in ids {
                    Self::settle_nft(&aid, id, block_number);
                }
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
            ensure!(current_block_number < nft.end, Error::<T>::BidClosed);

            if let Some(bid_price) = nft.bid_price {
            	ensure!(price > bid_price, Error::<T>::BidPriceTooLow);

                //竞标价格大于之前竞标人的价格
                //需要把之前竞标人的质押代币解锁
                let bidder = nft.bidder.unwrap();
                T::StakeCurrency::remove_lock(LOCK_ID, &bidder);
            }

            //竞标成功后，需要把竞标人的竞标价格的代币进行锁定
            T::StakeCurrency::set_lock(
                LOCK_ID,
                &sender,
                price,
                WithdrawReasons::all(),
            );

            nft.bid_price = Some(price);
            nft.bidder = Some(sender.clone());

            NFTs::<T>::insert(&nft_id, nft);

            Self::deposit_event(Event::Bidded { nft: nft_id, bidder: sender.clone(), bid_price: price });

        	Ok(())
        }

        ///手动结算某个NFT的竞标
        /// Withdrawal for one nft
        #[pallet::weight(0)]
        pub fn withdrawal(origin: OriginFor<T>, nft_id: [u8; 16]) -> DispatchResult {
        	// Make sure the caller is from a signed origin
            let sender = ensure_signed(origin)?;

            // Get the nft
            let nft = NFTs::<T>::get(&nft_id).ok_or(Error::<T>::NoNFT)?;

            let mut nft_clone = nft.clone();
            
            ensure!(!nft.settle, Error::<T>::NFTSettled);

            let from = nft.owner;

            ensure!(from == sender, Error::<T>::WithdrawalNotOwner);

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            ensure!(current_block_number >= nft.end, Error::<T>::BidNotClosed);

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

                    //解锁质押的代币
                    T::StakeCurrency::remove_lock(LOCK_ID, &to);

                    //转移代币
    	            T::StakeCurrency::transfer(&to, &from, bid_price, ExistenceRequirement::AllowDeath)?;

                    nft_clone.settle = true;

                    NFTs::<T>::insert(&nft_id, nft_clone);

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
            let end = Self::get_block_number();
            let nft = NFT::<T> { dna, bid_price: None, bidder: None, owner: owner.clone(), end, settle: false };

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

        //自动结算竞标结束的NFT
        pub fn settle_nft(owner: &T::AccountId,
                         nft_id: [u8; 16], 
                         block_number: T::BlockNumber,
        ) -> Result<(), DispatchError> {

            // Get the nft
            let nft = NFTs::<T>::get(&nft_id).ok_or(Error::<T>::NoNFT)?;

            let mut nft_clone = nft.clone();
            
            ensure!(!nft.settle, Error::<T>::NFTSettled);

            ensure!(*owner == nft.owner, Error::<T>::WithdrawalNotOwner);

            ensure!(block_number >= nft.end, Error::<T>::BidNotClosed);

            let mut from_owned = NFTsOwned::<T>::get(&owner);

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

                    //解锁质押的代币
                    T::StakeCurrency::remove_lock(LOCK_ID, &to);

                    //转移代币
                    T::StakeCurrency::transfer(&to, &owner, bid_price, ExistenceRequirement::KeepAlive)?;

                    nft_clone.settle = true;

                    NFTs::<T>::insert(&nft_id, nft_clone);

                    Self::deposit_event(Event::Withdrawal { nft: nft_id, from: owner.clone(), to: to.clone() });
                } else {
                    return Err(Error::<T>::NoBidder.into());
                }
            } else {
                return Err(Error::<T>::NoBidder.into());
            }

            Ok(())
        }
	}
}
