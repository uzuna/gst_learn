extern crate gstreamer as gst;
use anyhow::Context;
use env_logger::Env;
use gst::prelude::*;
use structopt::{clap::arg_enum, StructOpt};

fn tutorial_helloworld() -> anyhow::Result<()> {
    gst::init().context("failed to init gstreamer")?;

    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";

    let pipeline = gst::parse_launch(&format!("playbin uri={uri}")).context("failed to set uri")?;

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

    pipeline
        .set_state(gst::State::Null)
        .context("Unable to set the pipeline to the `Null` state")?;

    Ok(())
}

fn tutorial_concept() -> anyhow::Result<()> {
    gst::init().context("init")?;

    let source = gst::ElementFactory::make("videotestsrc", Some("source"))
        .context("Colud not create source element")?;
    let sink = gst::ElementFactory::make("autovideosink", Some("sink"))
        .context("Could not create sink element")?;

    let pipeline = gst::Pipeline::new(Some("test-pipeline"));

    pipeline
        .add_many(&[&source, &sink])
        .context("Add element to pipeline")?;
    source
        .link(&sink)
        .context("Elements could not be linked.")?;

    source.set_property_from_str("pattern", "smpte");

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

    pipeline
        .set_state(gst::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");

    Ok(())
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// show content of tutorial. B?=Basic
    #[structopt(possible_values = &Tutorial::variants(), case_insensitive = true)]
    tid: Tutorial,
}

arg_enum! {
    #[derive(Debug)]
    enum Tutorial {
        // Basic tutorial 1 HelloWorld
        B1,
        // Basic tutorial 2 Gstreamer concept
        B2,
    }
}

fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let opt = Opt::from_args();

    match opt.tid {
        Tutorial::B1 => tutorial_helloworld().unwrap(),
        Tutorial::B2 => tutorial_concept().unwrap(),
    }
}
