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

fn tutorial_dynamic_pipeline() -> anyhow::Result<()> {
    gst::init().context("init")?;

    let source =
        gst::ElementFactory::make("uridecodebin", Some("source")).context("make uridecodebin")?;
    let convert =
        gst::ElementFactory::make("audioconvert", Some("convert")).context("make audioconvert")?;
    let sink =
        gst::ElementFactory::make("autoaudiosink", Some("sink")).context("make audiosink")?;
    let resample =
        gst::ElementFactory::make("audioresample", Some("resample")).context("make resample")?;

    let pipeline = gst::Pipeline::new(None);
    pipeline
        .add_many(&[&source, &convert, &resample, &sink])
        .context("add element")?;

    // 音出力のラインだけ繋ぐ
    gst::Element::link_many(&[&convert, &resample, &sink])
        .context("Elements could not be linked.")?;

    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";
    source.set_property("uri", uri);

    // sourceにpadが作られた時のCallbackを登録
    // uriを追加したことでsrcとなるvideoとaudioのpadがここでみえる
    // audiopadだけを選択的に接続することで、映像無しで音声のみの出力がされる
    source.connect_pad_added(move |src, src_pad| {
        log::info!("Received new pad {} from {}", src_pad.name(), src.name());

        let sink_pad = convert
            .static_pad("sink")
            .expect("Failed to get static sink pad from convert");

        if sink_pad.is_linked() {
            log::info!("We are already linked.");
            return;
        }

        let new_pad_caps = src_pad
            .current_caps()
            .expect("Failed to get caps of new pad.");
        let new_pad_struct = new_pad_caps
            .structure(0)
            .expect("failed to get fiest structure");
        let new_pad_type = new_pad_struct.name();

        let is_audio = new_pad_type.starts_with("audio/x-raw");
        if !is_audio {
            log::info!(
                "It has type {} which is not raw audio. Ignoring.",
                new_pad_type
            );
            return;
        }

        let res = src_pad.link(&sink_pad);
        if res.is_err() {
            log::error!("Type is {} but link failed.", new_pad_type);
        } else {
            log::info!("Link succeeded (type {}).", new_pad_type);
        }
    });

    // start play
    pipeline
        .set_state(gst::State::Playing)
        .context("unable to set the pipeline to the `Playing` state")?;

    // check error, EOS, StateChange
    let bus = pipeline.bus().context("make bus")?;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Error(err) => {
                log::error!(
                    "Error received from element {:?} {} {:?}",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(state_changed) => {
                if state_changed.src().map(|s| s == pipeline).unwrap_or(false) {
                    log::info!(
                        "Pipeline state changed from {:?} to {:?}",
                        state_changed.old(),
                        state_changed.current()
                    );
                }
            }
            MessageView::Eos(_) => break,
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
        B3,
    }
}

fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let opt = Opt::from_args();

    match opt.tid {
        Tutorial::B1 => tutorial_helloworld().unwrap(),
        Tutorial::B2 => tutorial_concept().unwrap(),
        Tutorial::B3 => tutorial_dynamic_pipeline().unwrap(),
    }
}
