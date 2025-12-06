# BrowseMetadata Fix for Radio Paradise - PMO Music

**Date:** 2025-11-27
**Issue:** gupnp-av-cp failed to get metadata for live streams and history containers

## Problem

UPnP clients (like gupnp-av-cp) were unable to get metadata for:
- Live stream items (e.g., `radio-paradise:channel:mellow:live`)
- History containers (e.g., `radio-paradise:channel:mellow:history`)

Error:
```
Failed to get metadata for 'radio-paradise:channel:mellow:live'
Failed to get metadata for 'radio-paradise:channel:mellow:history'
```

## Root Cause

The UPnP ContentDirectory service has two browse modes:
- **BrowseMetadata**: Get metadata for a specific object (item or container)
- **BrowseDirectChildren**: Get the children of a container

The `ContentHandler::browse_metadata()` was calling `source.browse()` for all objects, but:
1. The `MusicSource::browse()` trait method is designed to return children, not object metadata
2. For leaf items (LiveStream, HistoryTrack), `RadioParadiseSource::browse()` was rejecting them as "cannot be browsed"
3. The trait doesn't provide a way to distinguish between BrowseMetadata and BrowseDirectChildren requests

## Solution

### 1. Modified ContentHandler ([pmomediaserver/src/content_handler.rs](pmomediaserver/src/content_handler.rs))

- `browse_metadata()` now tries `get_item()` first for leaf items before falling back to `browse()`
- This allows proper metadata retrieval for items (LiveStream, HistoryTrack)

### 2. Modified RadioParadiseSource ([pmoparadise/src/source.rs](pmoparadise/src/source.rs))

**For LiveStream items:**
- `browse()` now returns `BrowseResult::Items([live_item])` with the item's metadata
- This supports both BrowseMetadata (via ContentHandler) and direct browse calls

**For HistoryTrack items:**
- `browse()` now returns `BrowseResult::Items([track])` using `get_item()` internally
- Properly retrieves track metadata from the history playlist

**For History containers:**
- `browse()` now returns `BrowseResult::Mixed { containers: [history_container], items: [tracks] }`
- Provides both container metadata and its children in one result

### 3. Added Container Filtering ([pmomediaserver/src/content_handler.rs](pmomediaserver/src/content_handler.rs))

- `browse_result_to_didl()` now filters out containers that match the browsed `object_id`
- Prevents containers from appearing as children of themselves
- For History: BrowseDirectChildren returns only tracks, not the container

## Files Modified

1. ✅ [pmomediaserver/src/content_handler.rs](pmomediaserver/src/content_handler.rs)
   - Lines 120-135: Try get_item() first in browse_metadata()
   - Lines 290-310: Added object_id parameter and container filtering in browse_result_to_didl()
   - Lines 212, 286: Updated callers to pass object_id

2. ✅ [pmoparadise/src/source.rs](pmoparadise/src/source.rs)
   - Lines 345-369: Modified History browse to return Mixed (container + items)
   - Lines 371-377: Modified LiveStream browse to return item metadata
   - Lines 379-383: Modified HistoryTrack browse to return track metadata

## Validation

### Live Stream Metadata ✅
```bash
curl -X POST -H "SOAPAction: ..." BrowseMetadata radio-paradise:channel:mellow:live
```
Returns:
```xml
<item id="radio-paradise:channel:mellow:live" parentID="radio-paradise:channel:mellow">
  <dc:title>Unknown Title</dc:title>
  <upnp:class>object.item.audioItem.audioBroadcast</upnp:class>
  <res protocolInfo="http-get:*:audio/flac:*">http://.../radioparadise/stream/mellow/flac</res>
</item>
```

### History Container Metadata ✅
```bash
curl -X POST -H "SOAPAction: ..." BrowseMetadata radio-paradise:channel:mellow:history
```
Returns:
```xml
<container id="radio-paradise:channel:mellow:history" parentID="radio-paradise:channel:mellow">
  <dc:title>Mellow Mix - History</dc:title>
  <upnp:class>object.container.playlistContainer</upnp:class>
</container>
```

### History Children ✅
```bash
curl -X POST -H "SOAPAction: ..." BrowseDirectChildren radio-paradise:channel:mellow:history
```
Returns only track items (not the container itself)

## Design Notes

This solution works around a fundamental limitation in the `MusicSource` trait:
- The `browse()` method doesn't receive the `browse_flag` parameter
- It can't distinguish between BrowseMetadata and BrowseDirectChildren
- We use `get_item()` for items and `browse()` for containers
- Container filtering ensures correct BrowseDirectChildren behavior

## Testing Checklist

- [x] BrowseMetadata works for LiveStream items
- [x] BrowseMetadata works for History containers
- [x] BrowseDirectChildren works for History (returns only tracks)
- [x] Container filtering prevents self-reference
- [ ] Test with BubbleUPnP (user to verify)
- [ ] Test with gupnp-av-cp (user to verify)

## References

- Original issue report: [UPNP_FIX_SUMMARY.md](UPNP_FIX_SUMMARY.md)
- UPnP AV Architecture: https://openconnectivity.org/developer/specifications/upnp-resources/upnp/
