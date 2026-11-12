#![cfg(all(feature = "html", feature = "svg"))]

use std::fs;
use std::path::{Path, PathBuf};

use mupdf::text_page::TextBlockType;
use mupdf::{Document, Rect, TextPageFlags};

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

fn height(rect: Rect) -> f32 {
    rect.y1 - rect.y0
}

#[test]
fn reflow_landscape_image_moves_instead_of_shrinking() {
    // With room left on the page the image keeps its measured size.
    let body = format!(
        "{}<img src=\"land.svg\" width=\"600\" height=\"450\"></p>",
        spacer(250.0)
    );
    let pages = layout_case("landscape-fit", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 0, "the image should stay on the current page");
    assert_size(rect, 540.0, 405.0, 2.0);

    // Without room it moves to the next page at exactly the same size. The
    // old greedy fit shrank it into the remainder instead.
    let body = format!(
        "{}<img src=\"land.svg\" width=\"600\" height=\"450\"></p>",
        spacer(410.0)
    );
    let pages = layout_case("landscape-move", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 1, "the image should move to the next page");
    assert_size(rect, 540.0, 405.0, 2.0);
}

#[test]
fn reflow_portrait_image_moves_instead_of_shrinking() {
    let body = format!(
        "{}<img src=\"port.svg\" width=\"300\" height=\"600\"></p>",
        spacer(100.0)
    );
    let pages = layout_case("portrait-fit", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 0, "the image should stay on the current page");
    assert_size(rect, 300.0, 600.0, 2.0);

    let body = format!(
        "{}<img src=\"port.svg\" width=\"300\" height=\"600\"></p>",
        spacer(250.0)
    );
    let pages = layout_case("portrait-move", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 1, "the image should move to the next page");
    assert_size(rect, 300.0, 600.0, 2.0);
}

#[test]
fn reflow_image_taller_than_a_page_is_capped_and_placed_once() {
    // The measure pass caps an image to one page. flush_line must move that
    // line to a fresh page, not shrink it again or loop forever looking for
    // space that cannot exist.
    let body = format!(
        "{}<img src=\"port.svg\" width=\"300\" height=\"2000\"></p>",
        spacer(10.0)
    );
    let pages = layout_case("oversized", &body);
    let (page_no, rect) = first_image(&pages);
    assert_eq!(page_no, 1, "an oversized image should start a fresh page");
    let placed_h = height(rect);
    assert!(
        placed_h > 700.0 && placed_h <= 760.0,
        "the image should fill one page, got height {placed_h}"
    );
    let placed_w = rect.x1 - rect.x0;
    assert!(
        (placed_w / placed_h - 300.0 / 2000.0).abs() < 0.01,
        "the capped image should keep its aspect ratio"
    );
    assert!(
        pages.iter().flatten().count() == 1,
        "the image should be placed exactly once"
    );
}

#[test]
fn reflow_shared_image_line_moves_as_one_unit() {
    let body = format!(
        "{}<img src=\"land.svg\" width=\"150\" height=\"112\">\
         <img src=\"land.svg\" width=\"375\" height=\"188\"></p>",
        spacer(540.0)
    );
    let pages = layout_case("shared-line", &body);
    assert!(
        pages[0].is_empty(),
        "the line does not fit the remaining space"
    );
    let images = &pages[1];
    assert_eq!(
        images.len(),
        2,
        "both images should move together to the next page"
    );

    assert_size(images[0], 150.0, 112.0, 2.0);
    assert_size(images[1], 375.0, 188.0, 2.0);
    assert!(
        (images[0].y1 - images[1].y1).abs() <= 1.0,
        "images on a line should share a baseline"
    );
}

#[test]
fn reflow_adjacent_image_blocks_split_across_pages_keep_one_size() {
    // Two consecutive picture blocks of the same source image. Pagination may
    // split them across pages, but neither may be shrunk to the space left on
    // its page: in a spread that made one plate large and the other small,
    // and in the roll the image re-scaled as it slid into view.
    let body = format!(
        "{}<div><img src=\"land.svg\" width=\"600\" height=\"450\"></div>\
         <div><img src=\"land.svg\" width=\"600\" height=\"450\"></div>",
        spacer(450.0)
    );
    let pages = layout_case("adjacent-split", &body);
    assert!(
        pages[0].is_empty(),
        "the first image moves off the text page"
    );
    assert_eq!(pages[1].len(), 1);
    assert_eq!(pages[2].len(), 1);
    assert_size(pages[1][0], 540.0, 405.0, 2.0);
    assert_size(pages[2][0], 540.0, 405.0, 2.0);
    assert!(
        (height(pages[1][0]) - height(pages[2][0])).abs() <= 2.0,
        "both images must keep the same scale"
    );
}

#[test]
fn reflow_adjacent_images_keep_their_own_aspect_ratios() {
    // A run of adjacent images is not forced to a common size either: each
    // keeps the size its own bitmap and the text column imply.
    let body = format!(
        "{}<div><img src=\"land.svg\" width=\"600\" height=\"450\"></div>\
         <div><img src=\"port.svg\" width=\"300\" height=\"600\"></div>",
        spacer(450.0)
    );
    let pages = layout_case("adjacent-aspect", &body);
    assert_eq!(pages[1].len(), 1);
    assert_eq!(pages[2].len(), 1);
    assert_size(pages[1][0], 540.0, 405.0, 2.0);
    assert_size(pages[2][0], 300.0, 600.0, 2.0);
}
