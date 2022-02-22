extern crate gstreamer as gst;
use anyhow::Context;
use gst::prelude::*;

fn tutorial_helloworld() -> anyhow::Result<()> {
    gst::init().context("failed to init gstreamer")?;

    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";

    let pipeline =
        gst::parse_launch(&format!("playbin uri={uri}")).context("failed to set uri")?;

    pipeline
        .set_state(gst::State::Playing)
        .context("Unable to set the pipeline to the `Playing` state")?;

    let bus = pipeline.bus().context("fauled to get bus")?;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(_) => break,
            MessageView::Error(err) => {
                log::error!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()), 
                    err.error(),
                    err.debug()
                );
                break;
            }
            _ => {}
        }
    }

    pipeline.set_state(gst::State::Null).context("Unable to set the pipeline to the `Null` state")?;

    Ok(())
}

fn main() {
    tutorial_helloworld().unwrap();
}
