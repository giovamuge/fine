//! Enable Banking: the PSD2 account-information API that reads the user's bank data.

mod client;
mod error;
mod model;

pub use client::Client;
pub use error::{API_ERROR_PREFIX, ApiError, Error};
pub use model::{
    AccountId, AccountResource, Amount, Application, Aspsp, AuthResponse, Balance,
    BookedTransaction, Session, preferred_balance,
};
