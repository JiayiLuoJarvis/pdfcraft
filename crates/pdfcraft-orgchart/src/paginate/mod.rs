//! Pagination: cue slots, family search, and the session that numbers pages.

mod cues;
mod family;
mod session;

pub(crate) use session::paginate;
#[cfg(test)]
pub(crate) use session::{paginate_request, paginate_request_with};

#[cfg(test)]
mod chain_split_tests;
#[cfg(test)]
mod cue_tests;
#[cfg(test)]
mod place_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod whole_page_tests;
