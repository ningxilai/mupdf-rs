#![cfg(all(feature = "html", feature = "svg"))]

use std::fs;
use std::path::{Path, PathBuf};

use mupdf::text_page::TextBlockType;
use mupdf::{Document, Rect, TextPageFlags};

/// Golden-ratio share used by the reflow image placement heuristic.
const PHI: f32 = 0.618_034;

fn write_svg(dir: &Path, name: &str, width: u32, height: u32) {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\">\
         <rect width=\"{width}\" height=\"{height}\" fill=\"#c85050\"/></svg>"
    );
    fs::write(dir.join(name), svg).unwrap();
}

fn layout_case(name: &str, body: &str) -> Vec<Vec<Rect>> {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("mupdf-reflow-images-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    write_svg(&dir, "land.svg", 800, 600);
    write_svg(&dir, "port.svg", 400, 800);

    let html_path = dir.join("index.html");
    fs::write(
        &html_path,
        format!("<html><head><meta charset=\"utf-8\"></head><body>{body}</body></html>"),
    )
    .unwrap();

    let mut doc = Document::open(html_path.to_str().unwrap()).unwrap();
    doc.layout(612.0, 792.0, 12.0).unwrap();
    let page_count = doc.page_count().unwrap();

    let mut pages = Vec::new();
    for page_no in 0..page_count {
        let page = doc.load_page(page_no).unwrap();
        let text_page = page.to_text_page(TextPageFlags::PRESERVE_IMAGES).unwrap();
        let images = text_page
            .blocks()
            .filter(|block| block.r#type() == TextBlockType::Image)
            .map(|block| block.bounds())
            .collect();
        pages.push(images);
    }

    drop(doc);
    fs::remove_dir_all(&dir).ok();
    pages
}

fn first_image(pages: &[Vec<Rect>]) -> (usize, Rect) {
    for (page_no, images) in pages.iter().enumerate() {
        if let Some(rect) = images.first() {
            return (page_no, *rect);
        }
    }
    panic!("no image found on any page");
}

fn spacer(height: f32) -> String {
    format!(
        "<div style=\"height:{height}pt;margin:0;padding:0;border:0\"></div>\
         <p style=\"margin:0;text-align:center\">"
    )
}

fn assert_size(rect: Rect, width: f32, height: f32, tolerance: f32) {
    let actual_width = rect.x1 - rect.x0;
    let actual_height = rect.y1 - rect.y0;
    assert!(
        (actual_width - width).abs() <= tolerance,
        "width={actual_width}, expected={width}"
    );
    assert!(
        (actual_height - height).abs() <= tolerance,
        "height={actual_height}, expected={height}"
    );
}

#[test]
fn reflow_landscape_image_shrinks_only_when_wide_enough() {
    let body = format!(
        "{}<img src=\"land.svg\" width=\"600\" height=\"450\"></p>",
        spacer(410.0)
    );
    let pages = layout_case("landscape-shrink", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 0, "image should stay on the current page");
    let width = rect.x1 - rect.x0;
    let height = rect.y1 - rect.y0;
    assert!(
        width >= 540.0 * PHI,
        "scaled width={width} should still span the golden-ratio share of 540"
    );
    assert!(height < 405.0, "image should have been shrunk");
    assert!(height > 250.0, "image should not become a tiny strip");

    let body = format!(
        "{}<img src=\"land.svg\" width=\"600\" height=\"450\"></p>",
        spacer(520.0)
    );
    let pages = layout_case("landscape-move", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(
        page_no, 1,
        "too narrow an image should move to the next page"
    );
    assert_size(rect, 540.0, 405.0, 2.0);
}

#[test]
fn reflow_portrait_image_shrinks_only_when_tall_enough() {
    let body = format!(
        "{}<img src=\"port.svg\" width=\"300\" height=\"600\"></p>",
        spacer(250.0)
    );
    let pages = layout_case("portrait-shrink", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 0, "image should stay on the current page");
    let height = rect.y1 - rect.y0;
    assert!(
        height >= 720.0 * PHI,
        "scaled height={height} should still span the golden-ratio share of 720"
    );
    assert!(height < 600.0, "image should have been shrunk");

    let body = format!(
        "{}<img src=\"port.svg\" width=\"300\" height=\"600\"></p>",
        spacer(310.0)
    );
    let pages = layout_case("portrait-move", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 1, "a short remaining space should move the image");
    assert_size(rect, 300.0, 600.0, 2.0);
}

#[test]
fn reflow_shared_image_line_shrinks_images_individually() {
    let body = format!(
        "{}<img src=\"land.svg\" width=\"150\" height=\"112\">\
         <img src=\"land.svg\" width=\"375\" height=\"188\"></p>",
        spacer(540.0)
    );
    let pages = layout_case("shared-line", &body);
    let images = &pages[0];
    assert_eq!(
        images.len(),
        2,
        "both images should stay on the current page"
    );

    assert_size(images[0], 150.0, 112.0, 2.0);
    assert!(
        images[1].y1 - images[1].y0 < 188.0,
        "the taller image should be capped to the remaining height"
    );
    assert!(
        images[1].x1 - images[1].x0 > 320.0,
        "the taller image should still span most of the line width"
    );
    assert!(
        (images[0].y1 - images[1].y1).abs() <= 1.0,
        "images on a line should share a baseline"
    );
}
