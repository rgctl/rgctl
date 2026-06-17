//! Phase 14: web UI asset smoke tests.

#[test]
fn test_explorer_html_includes_d3() {
    let html = include_str!("../web/explorer.html");
    assert!(html.contains("d3js.org"));
    assert!(html.contains("explorer.js"));
}

#[test]
fn test_dashboard_html_includes_chartjs() {
    let html = include_str!("../web/dashboard.html");
    assert!(html.contains("chart.js"));
    assert!(html.contains("dashboard.js"));
}

#[test]
fn test_explorer_js_force_simulation() {
    let js = include_str!("../web/js/explorer.js");
    assert!(js.contains("forceSimulation"));
    assert!(js.contains("dblclick"));
}

#[test]
fn test_dashboard_js_community_centrality() {
    let js = include_str!("../web/js/dashboard.js");
    assert!(js.contains("complexity_histogram"));
    assert!(js.contains("/api/dashboard"));
    assert!(js.contains("/api/dashboard/advanced"));
    assert!(js.contains("community-chart"));
    assert!(js.contains("renderHotspots"));
    assert!(js.contains("renderCentralityChart"));
}

#[test]
fn test_dashboard_html_community_widgets() {
    let html = include_str!("../web/dashboard.html");
    assert!(html.contains("Community Sizes"));
    assert!(html.contains("connected-table"));
    assert!(html.contains("hotspots-table"));
    assert!(html.contains("communities-chart"));
    assert!(html.contains("centrality-chart"));
    assert!(html.contains("risk-badge"));
}

#[test]
fn test_index_links_to_phase14_pages() {
    let html = include_str!("../web/index.html");
    assert!(html.contains("explorer.html"));
    assert!(html.contains("dashboard.html"));
}

#[test]
fn test_main_index_uses_vis_network() {
    let html = include_str!("../web/index.html");
    assert!(html.contains("vis-network"));
}
