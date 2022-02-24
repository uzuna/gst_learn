extern crate gstreamer as gst;
use std::io::Write;

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

fn tutorial_queue() -> anyhow::Result<()> {
    struct CustomData {
        /// Our one and only element
        playbin: gst::Element,
        playing: bool,
        terminate: bool,
        seek_enabled: bool,
        seek_done: bool,
        duration: Option<gst::ClockTime>,
    }

    impl CustomData {
        fn new(playbin: gst::Element) -> Self {
            Self {
                playbin,
                playing: false,
                terminate: false,
                seek_enabled: false,
                seek_done: false,
                duration: gst::ClockTime::NONE,
            }
        }
    }

    fn handle_message(custom_data: &mut CustomData, msg: &gst::Message) -> anyhow::Result<()> {
        use gst::MessageView::*;

        match msg.view() {
            Error(err) => {
                log::error!(
                    "Error receive from Element {:?} {} {:?}",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug(),
                );
                custom_data.terminate = true;
            }
            Eos(_) => {
                log::info!("end of stream");
                custom_data.terminate = true;
            }
            DurationChanged(_) => {
                custom_data.duration = gst::ClockTime::NONE;
            }
            StateChanged(state_changed) => {
                if state_changed
                    .src()
                    .map(|s| s == custom_data.playbin)
                    .unwrap_or(false)
                {
                    let new_state = state_changed.current();
                    let old_state = state_changed.old();

                    log::info!(
                        "Pipeline state changed from {:?} to {:?}",
                        old_state,
                        new_state
                    );

                    custom_data.playing = new_state == gst::State::Playing;
                    if custom_data.playing {
                        // 再生が再開した時にSeekの状況がどうだったのかを確認する
                        // queryを使うことでパイプラインに情報を照会できる
                        let mut seeking = gst::query::Seeking::new(gst::Format::Time);
                        if custom_data.playbin.query(&mut seeking) {
                            let (seekable, start, end) = seeking.result();
                            custom_data.seek_enabled = seekable;
                            if seekable {
                                log::info!("Seeking is Enabled from {} to {}", start, end);
                            } else {
                                log::info!("Seeking is Distable for this stream");
                            }
                        } else {
                            log::error!("Seeking query failed")
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    gst::init().context("failed to init")?;
    let playbin = gst::ElementFactory::make("playbin", Some("playbin")).context("make playbin")?;
    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";
    playbin.set_property("uri", uri);
    playbin
        .set_state(gst::State::Playing)
        .context("set state playing")?;

    let bus = playbin.bus().context("bus")?;

    let mut custom_data = CustomData::new(playbin);

    while !custom_data.terminate {
        // メッセージの取得の制限時間を0.1秒とする
        let msg = bus.timed_pop(100 * gst::ClockTime::MSECOND);

        match msg {
            Some(msg) => {
                handle_message(&mut custom_data, &msg)?;
            }
            None => {
                // イベントが特にないなら通常通り更新する
                if custom_data.playing {
                    // query_positionで一夜基幹についt一般的な情報が得られる
                    let position = custom_data
                        .playbin
                        .query_position::<gst::ClockTime>()
                        .context("Could not query current position.")?;

                    if custom_data.duration == gst::ClockTime::NONE {
                        custom_data.duration = custom_data.playbin.query_duration();
                    }

                    log::info!("Position {} / {}", position, custom_data.duration.display());

                    std::io::stdout().flush().context("flush stdout")?;

                    // 再生状況を見て1度だけSeekイベントを発生させる
                    if custom_data.seek_enabled
                        && !custom_data.seek_done
                        && position > 3 * gst::ClockTime::SECOND
                    {
                        log::info!("Reached 10s, performing seek...");
                        // playbinに対して再生位置の指示を飛ばす
                        // GST_SEEK_FLAG_FLUSH: シークを実行する前に現在パイプラインにある全てのデータが破棄される。パイプラインにデータが流れるまで表示が一時停止するが、アプリケーションの応答性が良くなる。というか指定しないとPLAYINGなので破棄できなくて落ちる。
                        // GST_SEEK_FLAG_KEY_UNIT: ほとんどのビデオストリームは任意の位置を探せない。代わりにキーフレームには移動できる。これは最も近いキーフレームに移動する指示で基本的に他に選択肢はない。
                        // GST_SEEK_FLAG_ACCURATE: 一部メディアクリップは十分なインデックスがない事がありシーク位置を探すのに時間がかかる。Gstreamerは通常これを避けるために推定をするが位置精度が十分でない場合に正確な位置に飛ばしたい場合にこのフラグを立てる
                        custom_data
                            .playbin
                            .seek_simple(
                                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                20 * gst::ClockTime::SECOND,
                            )
                            .context("seek")?;
                        custom_data.seek_done = true;
                    }
                }
            }
        }
    }

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
        B4,
    }
}

fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let opt = Opt::from_args();

    match opt.tid {
        Tutorial::B1 => tutorial_helloworld().unwrap(),
        Tutorial::B2 => tutorial_concept().unwrap(),
        Tutorial::B3 => tutorial_dynamic_pipeline().unwrap(),
        Tutorial::B4 => tutorial_queue().unwrap(),
    }
}
