use crate::contentdirectory::variables::{
    A_ARG_TYPE_OBJECTID, A_ARG_TYPE_BROWSEFLAG, A_ARG_TYPE_FILTER,
    A_ARG_TYPE_SORTCRITERIA, A_ARG_TYPE_INDEX, A_ARG_TYPE_COUNT,
    A_ARG_TYPE_RESULT, A_ARG_TYPE_UPDATEID,
};
use pmoupnp::define_action;

define_action! {
    pub static BROWSE = "Browse" {
        in "ObjectID" => A_ARG_TYPE_OBJECTID,
        in "BrowseFlag" => A_ARG_TYPE_BROWSEFLAG,
        in "Filter" => A_ARG_TYPE_FILTER,
        in "StartingIndex" => A_ARG_TYPE_INDEX,
        in "RequestedCount" => A_ARG_TYPE_COUNT,
        in "SortCriteria" => A_ARG_TYPE_SORTCRITERIA,
        out "Result" => A_ARG_TYPE_RESULT,
        out "NumberReturned" => A_ARG_TYPE_COUNT,
        out "TotalMatches" => A_ARG_TYPE_COUNT,
        out "UpdateID" => A_ARG_TYPE_UPDATEID,
    }
}
