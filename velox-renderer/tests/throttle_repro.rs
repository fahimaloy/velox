// Repro for R-L3: rapid resize throttling — no raster surface per Resized event.
// This test FAILS before fix (Resized calls renderer.resize/presenter.resize / raster_n32_premul per event)
// and PASSES after fix (Resized only stores pending size, surface creation deferred to RedrawRequested).

#[test]
fn resized_does_not_create_surface_per_event() {
    let src = include_str!("../src/lib.rs");
    // Find every WindowEvent::Resized handler and ensure it does NOT
    // call surface creation / resize directly. Before fix each handler
    // contains `renderer.resize` and `presenter.resize` which allocate
    // a new raster surface (raster_n32_premul) per drag event.
    let mut found = 0usize;
    let mut offset = 0usize;
    while let Some(pos) = src[offset..].find("WindowEvent::Resized") {
        let abs = offset + pos;
        // Bound the handler to before its request_redraw (classic handler shape)
        // to avoid catching RedrawRequested's deferred resize.
        let window = &src[abs..(abs + 3500).min(src.len())];
        let bound = window.find("request_redraw").unwrap_or(window.len());
        let handler = &window[..bound];
        found += 1;
        assert!(
            !handler.contains("renderer.resize"),
            "Resized handler #{found} at offset {abs} still calls renderer.resize (must defer to RedrawRequested); handler snippet: {}",
            &handler[..handler.len().min(300)]
        );
        assert!(
            !handler.contains("presenter.resize"),
            "Resized handler #{found} at offset {abs} still calls presenter.resize (must defer to RedrawRequested)"
        );
        // Direct raster alloc must not happen in Resized either
        assert!(
            !handler.contains("raster_n32_premul"),
            "Resized handler #{found} still contains raster_n32_premul (per-event alloc)"
        );
        offset = abs + 1;
        if offset >= src.len() {
            break;
        }
    }
    assert!(
        found >= 2,
        "expected at least 2 Resized handlers (plain + HMR), found {found}"
    );
}

#[test]
fn redraw_deferred_surface_creation_via_pending_resize() {
    let src = include_str!("../src/lib.rs");
    // After fix there is a pending_resize coalescing variable and
    // RedrawRequested pulls it via .take() and then calls renderer.resize/presenter.resize once.
    assert!(
        src.contains("pending_resize"),
        "missing pending_resize coalescing variable — Resized must coalesce to last size per frame"
    );
    // Ensure at least the pattern `pending_resize.take()` or `pending_resize = Some` exists
    assert!(
        src.contains("pending_resize.take()") || src.contains("pending_resize .take()"),
        "pending_resize must be consumed in RedrawRequested via pending_resize.take()"
    );
    assert!(
        src.contains("pending_resize = Some"),
        "Resized handler must store pending_resize = Some(new_size)"
    );

    // Verify that RedrawRequested actually performs the deferred resize.
    // There should be a RedrawRequested block that contains both pending_resize.take() and renderer.resize
    let mut has_deferred_in_redraw = false;
    let mut offset = 0usize;
    while let Some(pos) = src[offset..].find("RedrawRequested") {
        let abs = offset + pos;
        let window = &src[abs..(abs + 4000).min(src.len())];
        if window.contains("pending_resize.take()") && window.contains("renderer.resize") {
            has_deferred_in_redraw = true;
            break;
        }
        offset = abs + 1;
        if offset >= src.len() {
            break;
        }
    }
    assert!(
        has_deferred_in_redraw,
        "RedrawRequested must handle pending_resize.take() and then call renderer.resize (deferred surface creation)"
    );
}

#[test]
fn drag_resize_coalesces_to_last_size_only() {
    // Simulate drag-resize coalescing: 100 rapid Resized events should
    // result in only the last size being materialized, not 100 allocations.
    // This models the pending_resize variable behavior.
    let mut pending: Option<(u32, u32)> = None;
    let drag_sizes = (0..100).map(|i| (800 + i, 600 + i));
    let mut surface_creations = 0usize;
    for (w, h) in drag_sizes {
        // Before fix: each Resized would call renderer.resize -> surface_creations +=1
        // After fix: only coalesce — store pending, no creation per event.
        pending = Some((w, h));
        // no creation here
    }
    // RedrawRequested fires once per frame
    if let Some((w, h)) = pending.take() {
        // deferred single creation with last size
        assert_eq!(
            (w, h),
            (899, 699),
            "coalesced pending must be last drag size"
        );
        surface_creations += 1;
    }
    assert_eq!(
        surface_creations, 1,
        "drag-resize of 100 events must coalesce to exactly 1 surface creation (got {surface_creations})"
    );

    // Also verify source does not create surface per event: the Resized handler
    // must not increment a counter per event — it only stores pending.
    let src = include_str!("../src/lib.rs");
    // If the fix is missing, this source check would have failed earlier,
    // but here we also assert that pending_resize appears at least twice
    // (store in Resized, take in RedrawRequested) to ensure coalescing.
    let store_count = src.matches("pending_resize = Some").count();
    let take_count = src.matches("pending_resize.take()").count();
    assert!(
        store_count >= 2,
        "expected at least 2 stores of pending_resize (plain + HMR), found {store_count}"
    );
    assert!(
        take_count >= 2,
        "expected at least 2 takes of pending_resize (plain + HMR), found {take_count}"
    );
}
