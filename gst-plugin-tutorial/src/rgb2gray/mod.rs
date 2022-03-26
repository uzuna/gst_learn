use gst::glib;
use gst::prelude::*;

mod imp;

// The public Rust wrapper type for our element
glib::wrapper! {
    pub struct Rgb2Gray(ObjectSubclass<imp::Rgb2Gray>) @extends gst_base::BaseTransform, gst::Element, gst::Object;
}

// Registers the type for our element, and then registers in GStreamer under
// the name "rsrgb2gray" for being able to instantiate it via e.g.
// gst::ElementFactory::make().
pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "rsrgb2gray",
        gst::Rank::None,
        Rgb2Gray::static_type(),
    )
}
