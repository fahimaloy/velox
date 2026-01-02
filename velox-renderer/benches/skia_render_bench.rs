use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

#[cfg(feature = "skia-native")]
use velox_dom::h;
#[cfg(feature = "skia-native")]
use velox_style::Stylesheet;

#[cfg(feature = "skia-native")]
fn build_repeated_boxes(count: usize) -> velox_dom::VNode {
    let mut children = Vec::with_capacity(count);
    for i in 0..count {
        let color = if i % 2 == 0 { "#FF5533" } else { "#3355FF" };
        let style = format!("background-color:{};width:8px;height:8px", color);
        children.push(h(
            "div",
            vec![("style", style.as_str())],
            vec![],
        ));
    }
    h("div", vec![("style", "width:256px;height:256px")], children)
}

#[cfg(feature = "skia-native")]
fn bench_render_boxes(c: &mut Criterion) {
    let mut group = c.benchmark_group("skia_render_boxes");
    group.sample_size(20);
    let sheet = Stylesheet::default();
    for &count in &[50usize, 200usize, 500usize] {
        let vnode = build_repeated_boxes(count);
        group.bench_with_input(BenchmarkId::from_parameter(count), &vnode, |b, v| {
            b.iter(|| {
                let _ = velox_renderer::render_vnode_to_raster_png(v, &sheet, 256, 256)
                    .expect("render");
            });
        });
    }
    group.finish();
}

#[cfg(not(feature = "skia-native"))]
fn bench_render_boxes(c: &mut Criterion) {
    c.bench_function("skia_render_boxes_disabled", |b| b.iter(|| ()));
}

criterion_group! {
    name = benches;
    config = Criterion::default().without_plots();
    targets = bench_render_boxes
}
criterion_main!(benches);
