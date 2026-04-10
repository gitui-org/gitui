//! cached lookups:
//! parts of the sync api that might take longer
//! to compute but change seldom so doing them async might be overkill

mod branchname;

pub use branchname::BranchName;
