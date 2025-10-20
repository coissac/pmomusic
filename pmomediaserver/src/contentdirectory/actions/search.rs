use crate::contentdirectory::handlers;
use crate::contentdirectory::variables::{
    A_ARG_TYPE_COUNT, A_ARG_TYPE_FILTER, A_ARG_TYPE_INDEX, A_ARG_TYPE_OBJECTID, A_ARG_TYPE_RESULT,
    A_ARG_TYPE_SEARCHCRITERIA, A_ARG_TYPE_SORTCRITERIA, A_ARG_TYPE_UPDATEID,
};
use pmoupnp::define_action;

define_action! {
    pub static SEARCH = "Search" stateless {
        in "ContainerID" => A_ARG_TYPE_OBJECTID,
        in "SearchCriteria" => A_ARG_TYPE_SEARCHCRITERIA,
        in "Filter" => A_ARG_TYPE_FILTER,
        in "StartingIndex" => A_ARG_TYPE_INDEX,
        in "RequestedCount" => A_ARG_TYPE_COUNT,
        in "SortCriteria" => A_ARG_TYPE_SORTCRITERIA,
        out "Result" => A_ARG_TYPE_RESULT,
        out "NumberReturned" => A_ARG_TYPE_COUNT,
        out "TotalMatches" => A_ARG_TYPE_COUNT,
        out "UpdateID" => A_ARG_TYPE_UPDATEID,
    }
    with handler handlers::search_handler()
}
