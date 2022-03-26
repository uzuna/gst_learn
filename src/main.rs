extern crate gstreamer as gst;
use std::{ffi::c_void, io::Write};

use anyhow::Context;
use env_logger::Env;
use glib::translate::IntoGlib;
use gst::{prelude::*, ResourceError};
use gstreamer_app::AppSink;
use structopt::StructOpt;

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

/// GTK GUIを通して表示する
/// Gstreamerに独自のウィンドウを作らせるのではなく特定のウィンドウに映像を出力する
/// Gstreamerからの情報で継続的にGUIを更新する
/// 複数のスレッドからGUIを更新する
/// 関心のあるメッセージをサブスクライブする
fn tutorial_guikit() -> anyhow::Result<()> {
    use std::process;

    use gdk::prelude::*;
    use gtk::prelude::*;

    use gstreamer_video::prelude::*;
    use std::ops;

    struct AppWindow {
        main_window: gtk::Window,
        timeout_id: Option<glib::SourceId>,
    }

    impl ops::Deref for AppWindow {
        type Target = gtk::Window;

        fn deref(&self) -> &gtk::Window {
            &self.main_window
        }
    }

    impl Drop for AppWindow {
        fn drop(&mut self) {
            if let Some(source_id) = self.timeout_id.take() {
                source_id.remove();
            }
        }
    }

    fn add_streams_info(playbin: &gst::Element, textbuf: &gtk::TextBuffer, stype: &str) {
        let propname = format!("n-{stype}");
        let signame = format!("get-{stype}-tags");

        let x = playbin.property::<i32>(&propname);
        for i in 0..x {
            let tags = playbin.emit_by_name::<Option<gst::TagList>>(&signame, &[&i]);

            if let Some(tags) = tags {
                textbuf.insert_at_cursor(&format!("{stype} stream {i}:\n"));
                if let Some(codec) = tags.get::<gst::tags::VideoCodec>() {
                    textbuf.insert_at_cursor(&format!("    codec: {} \n", codec.get()));
                }

                if let Some(codec) = tags.get::<gst::tags::AudioCodec>() {
                    textbuf.insert_at_cursor(&format!("    codec: {} \n", codec.get()));
                }

                if let Some(lang) = tags.get::<gst::tags::LanguageCode>() {
                    textbuf.insert_at_cursor(&format!("    language: {} \n", lang.get()));
                }

                if let Some(bitrate) = tags.get::<gst::tags::Bitrate>() {
                    textbuf.insert_at_cursor(&format!("    bitrate: {} \n", bitrate.get()));
                }
            }
        }
    }

    // Extract metadata from all the streams and write it to the text widget in the GUI
    fn analyze_streams(playbin: &gst::Element, textbuf: &gtk::TextBuffer) {
        {
            textbuf.set_text("");
        }
        add_streams_info(playbin, textbuf, "video");
        add_streams_info(playbin, textbuf, "audio");
        add_streams_info(playbin, textbuf, "text");
    }

    // This creates all the GTK+ widgets that compose our application, and registers the callbacks
    fn create_ui(playbin: &gst::Element) -> AppWindow {
        let main_window = gtk::Window::new(gtk::WindowType::Toplevel);
        main_window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });
        // GTK上にボタンを配置。名前、アイコン、イベントの登録
        let play_button =
            gtk::Button::from_icon_name(Some("media-playback-start"), gtk::IconSize::SmallToolbar);
        let pipeline = playbin.clone();
        play_button.connect_clicked(move |_| {
            let pipeline = &pipeline;
            pipeline
                .set_state(gst::State::Playing)
                .expect("unable to set the pipline to the `Playing` state");
        });

        let pause_button =
            gtk::Button::from_icon_name(Some("media-playback-pause"), gtk::IconSize::SmallToolbar);
        let pipeline = playbin.clone();
        pause_button.connect_clicked(move |_| {
            let pipeline = &pipeline;
            pipeline
                .set_state(gst::State::Paused)
                .expect("Unable to set the pipeline to the `Paused` state");
        });

        let stop_button =
            gtk::Button::from_icon_name(Some("media-playback-stop"), gtk::IconSize::SmallToolbar);
        let pipeline = playbin.clone();
        stop_button.connect_clicked(move |_| {
            let pipeline = &pipeline;
            // READYに遷移できるのはNull空だけだろ言うエラーが出た。Stopは本来どのような動作になるべき?
            pipeline
                .set_state(gst::State::Ready)
                .expect("Unable to set the pipeline to the `Ready` state");
        });

        let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        let pipeline = playbin.clone();
        let slider_update_signal_id = slider.connect_value_changed(move |slider| {
            let pipeline = &pipeline;
            let value = slider.value() as u64;
            if pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    value * gst::ClockTime::SECOND,
                )
                .is_err()
            {
                eprintln!("Seeking to {} failed", value);
            }
        });

        slider.set_draw_value(false);
        let pipeline = playbin.clone();
        let lslider = slider.clone();
        // Update the UI (seekbar) every second
        let timeout_id = glib::timeout_add_seconds_local(1, move || {
            let pipeline = &pipeline;
            let lslider = &lslider;

            if let Some(dur) = pipeline.query_duration::<gst::ClockTime>() {
                lslider.set_range(0.0, dur.seconds() as f64);

                if let Some(pos) = pipeline.query_position::<gst::ClockTime>() {
                    lslider.block_signal(&slider_update_signal_id);
                    lslider.set_value(pos.seconds() as f64);
                    lslider.unblock_signal(&slider_update_signal_id);
                }
            }
            Continue(true)
        });

        // ボタン配置
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        controls.pack_start(&play_button, false, false, 0);
        controls.pack_start(&pause_button, false, false, 0);
        controls.pack_start(&stop_button, false, false, 0);
        controls.pack_start(&slider, true, true, 2);

        // 表示エリアを作成
        let video_window = gtk::DrawingArea::new();

        // gstreanerとやり取りするためのGstVideoOverlayインターフェースでラップ
        // ここに画面のハンドルを渡すことで再生出来る
        let video_overlay = playbin
            .clone()
            .dynamic_cast::<gstreamer_video::VideoOverlay>()
            .unwrap();

        video_window.connect_realize(move |video_window| {
            let video_overlay = &video_overlay;
            let gdk_window = video_window.window().unwrap();

            if !gdk_window.ensure_native() {
                println!("Can't create native window for widget");
                process::exit(-1);
            }

            let display_type_name = gdk_window.display().type_().name();
            #[cfg(all(target_os = "linux", feature = "tutorial5-x11"))]
            {
                // Check if we're using X11 or ...
                if display_type_name == "GdkX11Display" {
                    extern "C" {
                        pub fn gdk_x11_window_get_xid(
                            window: *mut glib::object::GObject,
                        ) -> *mut c_void;
                    }

                    #[allow(clippy::cast_ptr_alignment)]
                    unsafe {
                        let xid = gdk_x11_window_get_xid(gdk_window.as_ptr() as *mut _);
                        video_overlay.set_window_handle(xid as usize);
                    }
                } else {
                    println!("Add support for display type '{}'", display_type_name);
                    process::exit(-1);
                }
            }
            #[cfg(all(target_os = "macos", feature = "tutorial5-quartz"))]
            {
                if display_type_name == "GdkQuartzDisplay" {
                    extern "C" {
                        pub fn gdk_quartz_window_get_nsview(
                            window: *mut glib::object::GObject,
                        ) -> *mut c_void;
                    }

                    #[allow(clippy::cast_ptr_alignment)]
                    unsafe {
                        let window = gdk_quartz_window_get_nsview(gdk_window.as_ptr() as *mut _);
                        video_overlay.set_window_handle(window as usize);
                    }
                } else {
                    println!(
                        "Unsupported display type '{}', compile with `--feature `",
                        display_type_name
                    );
                    process::exit(-1);
                }
            }
        });

        // ストリームの情報を表示する領域への弱参照を確保
        let streams_list = gtk::TextView::new();
        streams_list.set_editable(false);
        let pipeline_weak = playbin.downgrade();
        let streams_list_weak = glib::SendWeakRef::from(streams_list.downgrade());
        let bus = playbin.bus().unwrap();

        #[allow(clippy::single_match)]
        bus.connect_message(Some("application"), move |_, msg| match msg.view() {
            gst::MessageView::Application(application) => {
                let pipeline = match pipeline_weak.upgrade() {
                    Some(pipeline) => pipeline,
                    None => return,
                };

                let streams_list = match streams_list_weak.upgrade() {
                    Some(streams_list) => streams_list,
                    None => return,
                };

                if application.structure().map(|s| s.name()) == Some("tags-changed") {
                    let textbuf = streams_list
                        .buffer()
                        .expect("Couldn't get buffer from text_view");
                    analyze_streams(&pipeline, &textbuf);
                }
            }
            _ => unreachable!(),
        });

        let vbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        vbox.pack_start(&video_window, true, true, 0);
        vbox.pack_start(&streams_list, false, false, 2);

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.pack_start(&vbox, true, true, 0);
        main_box.pack_start(&controls, false, false, 0);
        main_window.add(&main_box);
        main_window.set_default_size(640, 480);

        main_window.show_all();

        AppWindow {
            main_window,
            timeout_id: Some(timeout_id),
        }
    }

    //メインスレッドにbusを通して通知?
    fn post_app_message(playbin: &gst::Element) {
        let _ = playbin.post_message(gst::message::Application::new(gst::Structure::new_empty(
            "tags-changed",
        )));
    }

    pub fn run() {
        // Make sure the right features were activated
        #[allow(clippy::eq_op)]
        {
            if !cfg!(feature = "tutorial5-x11") && !cfg!(feature = "tutorial5-quartz") {
                eprintln!(
                    "No Gdk backend selected, compile with --features tutorial5[-x11][-quartz]."
                );

                return;
            }
        }

        // Initialize GTK
        if let Err(err) = gtk::init() {
            eprintln!("Failed to initialize GTK: {}", err);
            return;
        }

        // Initialize GStreamer
        if let Err(err) = gst::init() {
            eprintln!("Failed to initialize Gst: {}", err);
            return;
        }

        // playbinはいつもどおり作成
        let uri = "https://www.freedesktop.org/software/gstreamer-sdk/\
                   data/media/sintel_trailer-480p.webm";
        let playbin = gst::ElementFactory::make("playbin", None).unwrap();
        playbin.set_property("uri", uri);

        // シグナルを取ってコールバックに流す
        playbin.connect("video-tags-changed", false, |args| {
            let pipeline = args[0]
                .get::<gst::Element>()
                .expect("playbin \"video-tags-changed\" args[0]");
            post_app_message(&pipeline);
            None
        });

        playbin.connect("audio-tags-changed", false, |args| {
            let pipeline = args[0]
                .get::<gst::Element>()
                .expect("playbin \"audio-tags-changed\" args[0]");
            post_app_message(&pipeline);
            None
        });

        playbin.connect("text-tags-changed", false, move |args| {
            let pipeline = args[0]
                .get::<gst::Element>()
                .expect("playbin \"text-tags-changed\" args[0]");
            post_app_message(&pipeline);
            None
        });

        let window = create_ui(&playbin);

        let bus = playbin.bus().unwrap();
        bus.add_signal_watch();

        let pipeline_weak = playbin.downgrade();
        bus.connect_message(None, move |_, msg| {
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return,
            };

            match msg.view() {
                //  This is called when an End-Of-Stream message is posted on the bus.
                // We just set the pipeline to READY (which stops playback).
                gst::MessageView::Eos(..) => {
                    println!("End-Of-Stream reached.");
                    pipeline
                        .set_state(gst::State::Ready)
                        .expect("Unable to set the pipeline to the `Ready` state");
                }

                // This is called when an error message is posted on the bus
                gst::MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                }
                // This is called when the pipeline changes states. We use it to
                // keep track of the current state.
                gst::MessageView::StateChanged(state_changed) => {
                    if state_changed.src().map(|s| s == pipeline).unwrap_or(false) {
                        println!("State set to {:?}", state_changed.current());
                    }
                }
                _ => (),
            }
        });

        playbin
            .set_state(gst::State::Playing)
            .expect("Unable to set the playbin to the `Playing` state");

        gtk::main();
        // 終了処理
        window.hide();
        playbin
            .set_state(gst::State::Null)
            .expect("Unable to set the playbin to the `Null` state");

        bus.remove_signal_watch();
    }
    run();

    Ok(())
}

/// 通常は自動的に処理されるPadについて
/// 取得の方法とタイミング
/// なぜPadについて知らなければならないか
fn tutorial_media_pad() -> anyhow::Result<()> {
    // 設定可能なCapabilityの一覧
    fn print_caps(caps: &gst::Caps, prefix: &str) {
        if caps.is_any() {
            log::info!("{prefix}ANY");
            return;
        }

        if caps.is_empty() {
            log::info!("{prefix}EMPTY");
            return;
        }

        for structure in caps.iter() {
            log::info!("{prefix}{}", structure.name());
            for (field, value) in structure.iter() {
                log::info!("{prefix} {field}:{}", value.serialize().unwrap().as_str());
            }
        }
    }
    // Elementの詳細を表示
    fn print_pad_template_information(factory: &gst::ElementFactory) {
        let long_name = factory
            .metadata("long-name")
            .expect("Failed to get long-name of element factory.");
        log::info!("Pad Template for {long_name}:");
        if factory.num_pad_templates() == 0u32 {
            log::info!("  None");
            return;
        }

        // padの情報を取り出す
        for pad_template in factory.static_pad_templates() {
            if pad_template.direction() == gst::PadDirection::Src {
                log::info!("  SRC template: '{}'", pad_template.name_template());
            } else if pad_template.direction() == gst::PadDirection::Sink {
                log::info!("  SINK template: '{}'", pad_template.name_template());
            } else {
                log::info!("  UNKNOWN!!! template: '{}'", pad_template.name_template());
            }
            if pad_template.presence() == gst::PadPresence::Always {
                log::info!("  Availability: Always");
            } else if pad_template.presence() == gst::PadPresence::Sometimes {
                log::info!("  Availability: Sometimes");
            } else if pad_template.presence() == gst::PadPresence::Request {
                log::info!("  Availability: On request");
            } else {
                log::info!("  Availability: UNKNOWN!!!");
            }

            let caps = pad_template.caps();
            log::info!("  Capabilities:");
            print_caps(&caps, "    ");
        }
    }

    fn print_pad_capabilities(element: &gst::Element, pad_name: &str) {
        let pad = element
            .static_pad(pad_name)
            .expect("Could not retrieve pad");

        log::info!("Caps for the {} pad:", pad_name);
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        print_caps(&caps, "      ");
    }

    // Initialize GStreamer
    gst::init().context("failed to init")?;

    // Create the element factories
    let source_factory = gst::ElementFactory::find("audiotestsrc")
        .context("Failed to create audiotestsrc factory.")?;
    let sink_factory = gst::ElementFactory::find("autoaudiosink")
        .context("Failed to create autoaudiosink factory.")?;

    // Print information about the pad templates of these factories
    print_pad_template_information(&source_factory);
    print_pad_template_information(&sink_factory);

    // Ask the factories to instantiate actual elements
    let source = source_factory
        .create(Some("source"))
        .context("Failed to create source element")?;
    let sink = sink_factory
        .create(Some("sink"))
        .context("Failed to create sink element")?;

    // Create the empty pipeline
    let pipeline = gst::Pipeline::new(Some("test-pipeline"));

    pipeline.add_many(&[&source, &sink]).unwrap();
    source
        .link(&sink)
        .context("Elements could not be linked.")?;

    // Print initial negotiated caps (in NULL state)
    log::info!("In NULL state:");
    print_pad_capabilities(&sink, "sink");

    // Start playing
    let res = pipeline.set_state(gst::State::Playing);
    if res.is_err() {
        log::error!(
            "Unable to set the pipeline to the `Playing` state (check the bus for error messages)."
        )
    }

    // Wait until error, EOS or State Change
    let bus = pipeline.bus().unwrap();

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Error(err) => {
                log::error!(
                    "Error received from element {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::Eos(..) => {
                log::info!("End-Of-Stream reached.");
                break;
            }
            MessageView::StateChanged(state_changed) =>
            // We are only interested in state-changed messages from the pipeline
            {
                if state_changed.src().map(|s| s == pipeline).unwrap_or(false) {
                    let new_state = state_changed.current();
                    let old_state = state_changed.old();

                    log::info!(
                        "Pipeline state changed from {:?} to {:?}",
                        old_state,
                        new_state
                    );
                    print_pad_capabilities(&sink, "sink");
                }
            }
            _ => (),
        }
    }

    // Shutdown pipeline
    pipeline
        .set_state(gst::State::Null)
        .context("Unable to set the pipeline to the `Null` state")?;

    Ok(())
}

/// パイプラインの一部の実行の新しいスレッドを作成する方法
/// パッドの可用性とは
/// ストリームの複製する方法
fn tutorial_multithread_pad() -> anyhow::Result<()> {
    // Gstreamはマルチスレッドフレームワーク。ストリーミングをアプリケーションスレッドから切り離すために内部でスレッドの作成と破棄をする。
    // プラグインは独自の処理用のスレッドを作ることも出来る
    // パイプライン小売クジもブランチが別のスレッドで実行されるように明示的に指定できる
    // ここではteeを通してvideoとaudioを別スレッドで処理する

    // Initialize GStreamer
    gst::init()?;

    let audio_source = gst::ElementFactory::make("audiotestsrc", Some("audio_source"))?;
    let tee = gst::ElementFactory::make("tee", Some("tee"))?;
    // queueが別スレッドで実行する受け役
    let audio_queue = gst::ElementFactory::make("queue", Some("audio_queue"))?;
    let audio_convert = gst::ElementFactory::make("audioconvert", Some("audio_convert"))?;
    let audio_resample = gst::ElementFactory::make("audioresample", Some("audio_resample"))?;
    let audio_sink = gst::ElementFactory::make("autoaudiosink", Some("audio_sink"))?;

    // 音声シグナルを波形表示に変換する
    let visual = gst::ElementFactory::make("wavescope", Some("visual"))?;
    let video_queue = gst::ElementFactory::make("queue", Some("video_queue"))?;
    let video_convert = gst::ElementFactory::make("videoconvert", Some("video_convert"))?;
    let video_sink = gst::ElementFactory::make("autovideosink", Some("video_sink"))?;

    let pipeline = gst::Pipeline::new(Some("pipeline"));

    // 生成波形の指定とbisualizerのパラメータ指定
    audio_source.set_property("freq", 440.0_f64);
    visual.set_property_from_str("shader", "none");
    visual.set_property_from_str("style", "lines");

    pipeline.add_many(&[
        &audio_source,
        &tee,
        &audio_queue,
        &audio_convert,
        &audio_resample,
        &audio_sink,
        &visual,
        &video_queue,
        &video_convert,
        &video_sink,
    ])?;

    // パイプラインをそれぞれ3スレッドでリンク
    gst::Element::link_many(&[&audio_source, &tee])?;
    gst::Element::link_many(&[&audio_queue, &audio_convert, &audio_resample, &audio_sink])?;
    gst::Element::link_many(&[&video_queue, &visual, &video_convert, &video_sink])?;

    // リクエストパッドを要求してQueueにリンクする
    let tee_audio_pad = tee.request_pad_simple("src_%u").context("tee_audio_pad")?;
    log::info!(
        "Obtained request pad {} for audio branch",
        tee_audio_pad.name()
    );
    let queue_audio_pad = audio_queue.static_pad("sink").context("queue_audio_pad")?;
    tee_audio_pad.link(&queue_audio_pad)?;

    let tee_video_pad = tee.request_pad_simple("src_%u").context("tee_video_pad")?;
    log::info!(
        "Obtained request pad {} for video branch",
        tee_audio_pad.name()
    );
    let queue_video_pad = video_queue.static_pad("sink").context("queue_video_pad")?;
    tee_video_pad.link(&queue_video_pad)?;

    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("bus")?;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView::*;
        match msg.view() {
            Error(err) => {
                log::error!(
                    "Error received from element {:?}: {} {:?}",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }

            Eos(..) => break,
            _ => (),
        }
    }

    pipeline
        .set_state(gst::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");

    Ok(())
}

/// 通常GStreamerは完全に閉じている必要はない
/// パイプラインに外からデータを注入する方法
/// パイプラインからデータを取り出す方法
/// データにアクセス、操作をする方法
fn tutorial_shortcut_pipeline() -> anyhow::Result<()> {
    // 幾つかの方法でパイプラインを流れるデータと対話出来る
    // アプリケーションデータをGStreamerに挿入するために使用する要素はappsrc
    // 出力のための要素はappsink
    // appsrcはPull or Pushモード、パイプライン下段主導か、独自のタイミングで出力するか選べる
    // このサンプルではPushモードとなる

    // データはバッファと呼ばれるチャンクでパイプラインを通過する。 `GstBuffers`
    // Srcで生成されてSinkで消費される
    // データの単位でしかないため、サイズ、タイムスタンプ、エレメントでのin/out個数は一定ではない
    // 今回の例ではANYキャップを使用してタイムスタンプを含まないバッファーを生成する
    // 逆にvideoとかはフレームを何時表示するのかを示す非常に正確なタイムスタンプがある

    use std::sync::{Arc, Mutex};

    use byte_slice_cast::*;

    use glib::source::SourceId;
    use gstreamer_app::{AppSink, AppSrc};
    use gstreamer_audio::AudioInfo;

    const CHUNK_SIZE: usize = 1024; // Amount of bytes we are sending in each buffer
    const SAMPLE_RATE: u32 = 44_100; // Samples per second we are sending

    #[derive(Debug)]
    struct CustomData {
        source_id: Option<SourceId>,

        // Number of samples generated so far(for tunestamp generation)
        num_samples: u64,
        // For waveforn generatuin
        a: f64,
        b: f64,
        c: f64,
        d: f64,

        appsrc: AppSrc,
        appsink: AppSink,
    }

    impl CustomData {
        fn new(appsrc: &AppSrc, appsink: &AppSink) -> Self {
            Self {
                source_id: None,
                num_samples: 0,
                a: 0.0,
                b: 1.0,
                c: 0.0,
                d: 1.0,
                appsrc: appsrc.clone(),
                appsink: appsink.clone(),
            }
        }
    }
    // Initialize GStreamer
    gst::init()?;

    let appsrc = gst::ElementFactory::make("appsrc", Some("audio_source"))?;
    let tee = gst::ElementFactory::make("tee", Some("tee"))?;
    // queueが別スレッドで実行する受け役
    let audio_queue = gst::ElementFactory::make("queue", Some("audio_queue"))?;
    let audio_convert1 = gst::ElementFactory::make("audioconvert", Some("audio_convert1"))?;
    let audio_resample = gst::ElementFactory::make("audioresample", Some("audio_resample"))?;
    let audio_sink = gst::ElementFactory::make("autoaudiosink", Some("audio_sink"))?;

    // 音声シグナルを波形表示に変換する
    let video_queue = gst::ElementFactory::make("queue", Some("video_queue"))?;
    let audio_convert2 = gst::ElementFactory::make("audioconvert", Some("audio_convert2"))?;
    let visual = gst::ElementFactory::make("wavescope", Some("visual"))?;
    let video_convert = gst::ElementFactory::make("videoconvert", Some("video_convert"))?;
    let video_sink = gst::ElementFactory::make("autovideosink", Some("video_sink"))?;

    // appsinkに流す
    let app_queue = gst::ElementFactory::make("queue", Some("app_queue"))?;
    let appsink = gst::ElementFactory::make("appsink", Some("app_sink"))?;

    let pipeline = gst::Pipeline::new(Some("pipeline"));
    visual.set_property_from_str("shader", "none");
    visual.set_property_from_str("style", "lines");

    // add pipeline
    pipeline.add_many(&[
        &appsrc,
        &tee,
        &audio_queue,
        &audio_convert1,
        &audio_resample,
        &audio_sink,
        &video_queue,
        &audio_convert2,
        &visual,
        &video_convert,
        &video_sink,
        &app_queue,
        &appsink,
    ])?;
    gst::Element::link_many(&[&appsrc, &tee])?;
    gst::Element::link_many(&[&audio_queue, &audio_convert1, &audio_resample, &audio_sink])?;
    gst::Element::link_many(&[
        &video_queue,
        &audio_convert2,
        &visual,
        &video_convert,
        &video_sink,
    ])?;
    gst::Element::link_many(&[&app_queue, &appsink])?;

    fn link_pad(
        src: &gst::Element,
        dst: &gst::Element,
    ) -> Result<gst::PadLinkSuccess, gst::PadLinkError> {
        let src_pad = src.request_pad_simple("src_%u").unwrap();
        log::info!("Obtained request pad {} for audio branch", src_pad.name());

        let dst_pad = dst.static_pad("sink").unwrap();
        src_pad.link(&dst_pad)
    }
    link_pad(&tee, &audio_queue)?;
    link_pad(&tee, &video_queue)?;
    link_pad(&tee, &app_queue)?;

    // configure appsrc

    let info = AudioInfo::builder(gstreamer_audio::AudioFormat::S16le, SAMPLE_RATE, 1).build()?;
    let audio_caps = info.to_caps()?;

    let appsrc = appsrc.dynamic_cast::<AppSrc>().unwrap();
    appsrc.set_caps(Some(&audio_caps));
    appsrc.set_format(gst::Format::Time);

    let appsink = appsink.dynamic_cast::<AppSink>().unwrap();
    let data = Arc::new(Mutex::new(CustomData::new(&appsrc, &appsink)));
    let data_weak = Arc::downgrade(&data);
    let data_weak2 = Arc::downgrade(&data);

    // appsrcにシグナルコールバックを登録する
    // need-data, enough-dataでそれぞれデータが空になるか、いっぱいになるかで発火する
    // need-dataではデータがほぼ空になったらデータを生成してappsinkのバッファーに積む
    // enough-dataが呼ばれたら登録されたsource_idを使ってfeeding処理を停止する
    appsrc.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |_, _| {
                let data = match data_weak.upgrade() {
                    Some(data) => data,
                    None => return,
                };
                let mut d = data.lock().unwrap();

                if d.source_id.is_none() {
                    log::info!("start feeding");
                    // 2つめのdowngradeを用意してidle_addで別のロックを取った結果を書き込ませる?
                    // 競合しないの?
                    let data_weak = Arc::downgrade(&data);
                    // idle_addはデータをフィードするためのアイドル関数
                    // 他に優先度の高いタスクがない時にこの処理が呼ばれる
                    d.source_id = Some(glib::source::idle_add(move || {
                        let data = match data_weak.upgrade() {
                            Some(data) => data,
                            None => return glib::Continue(false),
                        };

                        let (appsrc, buffer) = {
                            let mut data = data.lock().unwrap();
                            let mut buffer = gst::Buffer::with_size(CHUNK_SIZE).unwrap();
                            let num_samples = CHUNK_SIZE / 2; /* Each sample is 16 bits */
                            let pts = gst::ClockTime::SECOND
                                .mul_div_floor(data.num_samples, u64::from(SAMPLE_RATE))
                                .expect("u64 overflow");
                            let duration = gst::ClockTime::SECOND
                                .mul_div_floor(num_samples as u64, u64::from(SAMPLE_RATE))
                                .expect("u64 overflow");

                            {
                                let buffer = buffer.get_mut().unwrap();
                                {
                                    let mut samples = buffer.map_writable().unwrap();
                                    let samples = samples.as_mut_slice_of::<i16>().unwrap();

                                    // Generate some psychodelic waveforms
                                    data.c += data.d;
                                    data.d -= data.c / 1000.0;
                                    let freq = 1100.0 + 1000.0 * data.d;

                                    for sample in samples.iter_mut() {
                                        data.a += data.b;
                                        data.b -= data.a / freq;
                                        *sample = 500 * (data.a as i16);
                                    }

                                    data.num_samples += num_samples as u64;
                                }

                                buffer.set_pts(pts);
                                buffer.set_duration(duration);
                            }

                            (data.appsrc.clone(), buffer)
                        };

                        glib::Continue(appsrc.push_buffer(buffer).is_ok())
                    }));
                }
            })
            .enough_data(move |_| {
                let data = match data_weak2.upgrade() {
                    Some(data) => data,
                    None => return,
                };

                let mut data = data.lock().unwrap();
                if let Some(source) = data.source_id.take() {
                    log::info!("stop feeding {source:?}");
                    source.remove();
                }
            })
            .build(),
    );

    // configure appsink
    appsink.set_caps(Some(&audio_caps));

    let data_weak = Arc::downgrade(&data);
    // appsinkのcallbackでnew_sampleは新しいバッファが来るたびに発行される
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |_| {
                let data = match data_weak.upgrade() {
                    Some(data) => data,
                    None => return Ok(gst::FlowSuccess::Ok),
                };

                let appsink = {
                    let data = data.lock().unwrap();
                    data.appsink.clone()
                };

                if let Ok(_sample) = appsink.pull_sample() {
                    // Sample: https://docs.rs/gstreamer/latest/gstreamer/sample/struct.Sample.html
                    // has buffer(data detail), caps(format), segment(timestamp)
                    // The only thing we do in this example is print a * to indicate a received buffer
                    print!("*");
                    let _ = std::io::stdout().flush();
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();
    let bus = pipeline.bus().unwrap();
    #[allow(clippy::single_match)]
    bus.connect_message(Some("error"), move |_, msg| match msg.view() {
        gst::MessageView::Error(err) => {
            let main_loop = &main_loop_clone;
            log::error!(
                "Error received from element {:?}: {} {:?}",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug(),
            );
            main_loop.quit();
        }
        _ => unreachable!(),
    });
    bus.add_signal_watch();

    pipeline
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state.");

    main_loop.run();

    pipeline
        .set_state(gst::State::Null)
        .expect("Unable to set the pipeline to the `Null` state.");

    bus.remove_signal_watch();

    Ok(())
}

/// URIに関する情報を復元する方法
/// URIが再生可能課確認する方法
fn tutorial_media_info(uri: &str) -> anyhow::Result<()> {
    // GstDiscoverのpbutilsで１つ以上のURIを受け取ってそれらに関する情報を得られる
    // 同期モードで呼び出す場合はgst_discoverer_discover_uri()
    // 非同期の場合は以下のチュートリアルで行う。
    // 復元できるのはCodec, Stream topology, available Metadataが含まれる
    // gst-discover-1.0が同じことをしている

    use gstreamer_pbutils::{
        prelude::*, Discoverer, DiscovererContainerInfo, DiscovererInfo, DiscovererResult,
        DiscovererStreamInfo,
    };

    fn send_value_as_str(v: &glib::SendValue) -> Option<String> {
        if let Ok(s) = v.get::<&str>() {
            Some(s.to_string())
        } else if let Ok(serialized) = v.serialize() {
            Some(serialized.into())
        } else {
            None
        }
    }

    fn print_stream_info(info: &DiscovererStreamInfo, depth: usize) {
        let caps_str = if let Some(caps) = info.caps() {
            if caps.is_fixed() {
                gstreamer_pbutils::pb_utils_get_codec_description(&caps)
                    .unwrap_or_else(|_| glib::GString::from("unknown codec"))
            } else {
                glib::GString::from(caps.to_string())
            }
        } else {
            glib::GString::from("")
        };

        let stream_nick = info.stream_type_nick();
        log::info!(
            "{stream_nick:>indent$}: {caps_str}",
            stream_nick = stream_nick,
            indent = 2 * depth + stream_nick.len(),
            caps_str = caps_str
        );

        if let Some(tags) = info.tags() {
            log::info!("{:indent$}Tags:", " ", indent = 2 * depth);
            for (tag, values) in tags.iter_generic() {
                let mut tags_str = format!(
                    "{tag:>indent$}: ",
                    tag = tag,
                    indent = 2 * (2 + depth) + tag.len()
                );
                let mut tag_num = 0;
                for value in values {
                    if let Some(s) = send_value_as_str(value) {
                        if tag_num > 0 {
                            tags_str.push_str(", ")
                        }
                        tags_str.push_str(&s[..]);
                        tag_num += 1;
                    }
                }
                log::info!("{tags_str}");
            }
        }
    }

    fn print_topology(info: &DiscovererStreamInfo, depth: usize) {
        print_stream_info(info, depth);

        if let Some(next) = info.next() {
            print_topology(&next, depth + 1);
        } else if let Some(container_info) = info.downcast_ref::<DiscovererContainerInfo>() {
            for stream in container_info.streams() {
                print_topology(&stream, depth + 1);
            }
        }
    }

    fn on_discovered(
        _discoverer: &Discoverer,
        discoverer_info: &DiscovererInfo,
        error: Option<&glib::Error>,
    ) {
        let uri = discoverer_info.uri().unwrap();
        match discoverer_info.result() {
            DiscovererResult::Ok => log::info!("Discovered {uri}"),
            DiscovererResult::UriInvalid => log::info!("Invalid uri {uri}"),
            DiscovererResult::Error => {
                if let Some(msg) = error {
                    log::info!("{msg}");
                } else {
                    log::info!("Unknown error")
                }
            }
            DiscovererResult::Timeout => log::info!("Timeout"),
            DiscovererResult::Busy => log::info!("Busy"),
            DiscovererResult::MissingPlugins => {
                if let Some(s) = discoverer_info.misc() {
                    log::info!("{}", s);
                }
            }
            _ => log::info!("Unknown result"),
        }

        if discoverer_info.result() != DiscovererResult::Ok {
            return;
        }

        log::info!("Duration: {}", discoverer_info.duration().display());

        if let Some(tags) = discoverer_info.tags() {
            log::info!("Tags:");
            for (tag, values) in tags.iter_generic() {
                values.for_each(|v| {
                    if let Some(s) = send_value_as_str(v) {
                        log::info!("  {tag}: {s}")
                    }
                })
            }
        }

        log::info!(
            "Seekable: {}",
            if discoverer_info.is_seekable() {
                "yes"
            } else {
                "no"
            }
        );

        log::info!("Stream information:");

        if let Some(stream_info) = discoverer_info.stream_info() {
            print_topology(&stream_info, 1);
        }
    }

    log::info!("Discovering {uri}");

    gst::init()?;

    let loop_ = glib::MainLoop::new(None, false);
    let timeout = 5 * gst::ClockTime::SECOND;
    let discoverer = gstreamer_pbutils::Discoverer::new(timeout)?;
    discoverer.connect_discovered(on_discovered);
    let loop_clone = loop_.clone();
    discoverer.connect_finished(move |_| {
        log::info!("Finished discovering");
        loop_clone.quit();
    });
    discoverer.start();
    discoverer.discover_uri_async(uri)?;
    loop_.run();

    discoverer.stop();

    Ok(())
}

/// bufferingを有効にする方法(ネットワークの問題の軽減)
/// 中断から回復する方法
fn tutorial_streaming() -> anyhow::Result<()> {
    gst::init()?;

    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";
    let pipeline = gst::parse_launch(&format!("playbin uri={}", uri))?;

    // Start playing
    let res = pipeline.set_state(gst::State::Playing)?;
    let is_live = res == gst::StateChangeSuccess::NoPreroll;

    let main_loop = glib::MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();
    let pipeline_weak = pipeline.downgrade();
    let bus = pipeline.bus().expect("Pipeline has no bus");
    bus.add_watch(move |_, msg| {
        use gst::MessageView::*;
        let pipeline = match pipeline_weak.upgrade() {
            Some(pipeline) => pipeline,
            None => return glib::Continue(true),
        };
        let main_loop = &main_loop_clone;

        match msg.view() {
            Error(err) => {
                log::error!(
                    "Error received from element {:?}: {} {:?}",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug(),
                );
                main_loop.quit();
            }
            Eos(_) => {
                // end-of-stream
                let _ = pipeline.set_state(gst::State::Ready);
                main_loop.quit();
            }
            // bufferが所定量貯まるまで再生しない
            Buffering(buffering) => {
                if is_live {
                    return glib::Continue(true);
                }
                let percent = buffering.percent();
                log::info!("Buffering ({percent})");
                std::io::stdout().flush().unwrap();

                if percent < 30 {
                    let _ = pipeline.set_state(gst::State::Paused);
                } else {
                    let _ = pipeline.set_state(gst::State::Playing);
                }
            }
            ClockLost(_) => {
                // Get a new clock
                let _ = pipeline.set_state(gst::State::Paused);
                let _ = pipeline.set_state(gst::State::Playing);
            }
            _ => {}
        }
        glib::Continue(true)
    })?;

    main_loop.run();

    bus.remove_watch()?;
    pipeline.set_state(gst::State::Null)?;

    Ok(())
}

/// 再生速度を変化させる方法
/// ビデオをフレームごとに進める方法
fn tutorial_playback_speed() -> anyhow::Result<()> {
    // 再生速度の変化、逆再生についても再生レートで制御できる
    // 再生速度の変更方法はステップイベントとシークイベントの2種類がある
    // ステップイベントは主に1以上の高速再生でメディアをスキップするのに
    // シークイベントは逆再生も含めて任意の位置にジャンプするのに使う
    // ステップイベントは少ない設定で出来る変わりに行くるか制約があるため例ではシークイベントを使う

    use gst::event::{Seek, Step};
    use gst::prelude::*;
    use gst::{Element, SeekFlags, SeekType, State};

    use anyhow::Error;

    use termion::event::Key;
    use termion::input::TermRead;
    use termion::raw::IntoRawMode;

    use std::{io, thread, time};

    #[derive(Clone, Copy, PartialEq)]
    enum Command {
        PlayPause,
        DataRateUp,
        DataRateDown,
        ReverseRate,
        NextFrame,
        Quit,
    }

    fn send_seek_event(pipeline: &Element, rate: f64) -> bool {
        let position = match pipeline.query_position() {
            Some(pos) => pos,
            None => {
                eprintln!("Unable to retrieve current position...\r");
                return false;
            }
        };

        // seekはワーニングが出ていて出来なかった
        // matroska-demux.c:2953:gst_matroska_demux_handle_seek_push:<matroskademux0> Seek end-time not supported in streaming mode
        let seek_event = if rate > 0. {
            Seek::new(
                rate,
                SeekFlags::FLUSH | SeekFlags::ACCURATE,
                SeekType::Set,
                position,
                SeekType::End,
                gst::ClockTime::ZERO,
            )
        } else {
            Seek::new(
                rate,
                SeekFlags::FLUSH | SeekFlags::ACCURATE,
                SeekType::Set,
                position,
                SeekType::Set,
                position,
            )
        };

        // If we have not done so, obtain the sink through which we will send the seek events
        if let Ok(Some(video_sink)) = pipeline.try_property::<Option<Element>>("video-sink") {
            println!("Current rate: {}\r", rate);
            // Send the event
            let r = video_sink.send_event(seek_event);
            if !r {
                log::warn!("failed to set seek event");
            }

            r
        } else {
            eprintln!("Failed to update rate...\r");
            false
        }
    }

    fn handle_keyboard(ready_tx: glib::Sender<Command>) {
        // We set the terminal in "raw mode" so that we can get the keys without waiting for the user
        // to press return.
        let _stdout = io::stdout().into_raw_mode().unwrap();
        let mut stdin = termion::async_stdin().keys();

        loop {
            if let Some(Ok(input)) = stdin.next() {
                let command = match input {
                    Key::Char('p' | 'P') => Command::PlayPause,
                    Key::Char('s') => Command::DataRateDown,
                    Key::Char('S') => Command::DataRateUp,
                    Key::Char('d' | 'D') => Command::ReverseRate,
                    Key::Char('n' | 'N') => Command::NextFrame,
                    Key::Char('q' | 'Q') => Command::Quit,
                    Key::Ctrl('c' | 'C') => Command::Quit,
                    _ => continue,
                };
                ready_tx
                    .send(command)
                    .expect("failed to send data through channel");
                if command == Command::Quit {
                    break;
                }
            }
            thread::sleep(time::Duration::from_millis(50));
        }
    }

    gst::init()?;

    // Print usage map.
    println!(
        "\
USAGE: Choose one of the following options, then press enter:
 'P' to toggle between PAUSE and PLAY
 'S' to increase playback speed, 's' to decrease playback speed
 'D' to toggle playback direction
 'N' to move to next frame (in the current direction, better in PAUSE)
 'Q' to quit"
    );

    // Get a main context...
    let main_context = glib::MainContext::default();
    // ... and make it the main context by default so that we can then have a channel to send the
    // commands we received from the terminal.
    let _guard = main_context.acquire().unwrap();

    // Build the channel to get the terminal inputs from a different thread.
    let (ready_tx, ready_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    thread::spawn(move || handle_keyboard(ready_tx));

    // Build the pipeline.
    let uri =
        "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm";
    let pipeline = gst::parse_launch(&format!("playbin uri={}", uri))?;

    // Start playing.
    let _ = pipeline.set_state(State::Playing)?;
    let main_loop = glib::MainLoop::new(Some(&main_context), false);
    let main_loop_clone = main_loop.clone();
    let pipeline_weak = pipeline.downgrade();
    let mut playing = true;
    let mut rate = 1.;

    ready_rx.attach(Some(&main_loop.context()), move |command: Command| {
        use Command::*;
        let pipeline = match pipeline_weak.upgrade() {
            Some(pipeline) => pipeline,
            None => return glib::Continue(true),
        };

        match command {
            PlayPause => {
                let status = if playing {
                    let _ = pipeline.set_state(State::Paused);
                    "PAUSE"
                } else {
                    let _ = pipeline.set_state(State::Playing);
                    "PLAYING"
                };
                playing = !playing;
                println!("Setting state to {}\r", status);
            }
            DataRateUp => {
                if send_seek_event(&pipeline, rate * 2.) {
                    rate *= 2.;
                }
            }
            DataRateDown => {
                if send_seek_event(&pipeline, rate / 2.) {
                    rate /= 2.;
                }
            }
            ReverseRate => {
                if send_seek_event(&pipeline, rate * -1.) {
                    rate *= -1.;
                }
            }
            NextFrame => {
                if let Ok(Some(video_sink)) = pipeline.try_property::<Option<Element>>("video-sink")
                {
                    // Send the event
                    let step = Step::new(gst::format::Buffers(1), rate.abs(), true, false);
                    video_sink.send_event(step);
                    println!("Stepping one frame\r");
                }
            }
            Quit => {
                main_loop_clone.quit();
            }
        }

        glib::Continue(true)
    });
    main_loop.run();

    pipeline.set_state(State::Null)?;

    Ok(())
}

/// videotestsrcのプレビューとメタデータの表示を行う
fn preview_metadata() -> anyhow::Result<()> {
    gst::init()?;

    let source = gst::ElementFactory::make("videotestsrc", Some("source"))
        .context("Colud not create source element")?;
    let timeoverlay = gst::ElementFactory::make("timeoverlay", Some("timeoverlay"))?;
    let tee = gst::ElementFactory::make("tee", Some("tee"))?;
    let prev_queue = gst::ElementFactory::make("queue", Some("prev_queue"))?;
    let app_queue = gst::ElementFactory::make("queue", Some("app_queue"))?;
    let prev_sink = gst::ElementFactory::make("autovideosink", Some("sink"))?;
    let app_sink = gst::ElementFactory::make("appsink", Some("appsink"))?;

    let pipeline = gst::Pipeline::new(Some("test-pipeline"));

    pipeline.add_many(&[
        &source,
        &timeoverlay,
        &tee,
        &prev_queue,
        &prev_sink,
        &app_queue,
        &app_sink,
    ])?;

    fn link_pad(
        src: &gst::Element,
        dst: &gst::Element,
    ) -> Result<gst::PadLinkSuccess, gst::PadLinkError> {
        let src_pad = src.request_pad_simple("src_%u").unwrap();
        log::info!("Obtained request pad {} for audio branch", src_pad.name());

        let dst_pad = dst.static_pad("sink").unwrap();
        src_pad.link(&dst_pad)
    }
    gst::Element::link_many(&[&source, &timeoverlay, &tee])?;
    gst::Element::link_many(&[&prev_queue, &prev_sink])?;
    gst::Element::link_many(&[&app_queue, &app_sink])?;
    link_pad(&tee, &prev_queue)?;
    link_pad(&tee, &app_queue)?;

    let app_sink = app_sink.dynamic_cast::<AppSink>().unwrap();
    app_sink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |app_sink| {
                if let Ok(sample) = app_sink.pull_sample() {
                    log::info!(
                        "Buffer: {:?}, Caps: {:?}, Segment: {:?} BT:{:?}",
                        sample.buffer().unwrap(),
                        sample.caps().unwrap(),
                        sample.segment().unwrap(),
                        app_sink.base_time().unwrap()
                    );
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    source.set_property_from_str("pattern", "smpte");
    // 意味はわからないけど設定出来る
    // source.set_property("blocksize", 10_u32);
    // live sourceならばtimestamp付与が出来るが、どこにどのように付与されているのかはわからなかった
    source.set_property("is-live", true);
    source.set_property("do-timestamp", true);

    pipeline
        .set_state(gst::State::Playing)
        .context("Unable to set the pipeline to the `Playing` state")?;

    let bus = pipeline.bus().context("fauled to get bus")?;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(_) => break,
            MessageView::Error(err) => {
                // window close -> "Output window was closed"
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
    #[structopt(subcommand)]
    tid: Tutorial,
}

#[derive(Debug, StructOpt)]
enum Tutorial {
    /// Basic tutorial 1 HelloWorld
    B1,
    /// Basic tutorial 2 Gstreamer concept
    B2,
    /// Basic tutorial 3 Dynamic pipeline
    B3,
    /// Basic tutorial 4 time managgement
    B4,
    /// Basic tutorial 5 GUI toolkit
    B5,
    /// Basic tutorial 6 Media format and pads
    B6,
    /// Basic tutorial 7 Multithread
    B7,
    /// Basic tutorial 8 shuort-cutting the pipeline
    B8,
    /// Basic tutorial 9 Discover
    B9 {
        #[structopt(
            default_value = "https://www.freedesktop.org/software/gstreamer-sdk/data/media/sintel_trailer-480p.webm"
        )]
        uri: String,
    },
    // Basic tutorial 12 Buffering
    B12,
    // Basic tutorial 13 PlaybackSpeed
    B13,

    // test metadata view
    T1,
}
fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let opt = Opt::from_args();

    match opt.tid {
        Tutorial::B1 => tutorial_helloworld().unwrap(),
        Tutorial::B2 => tutorial_concept().unwrap(),
        Tutorial::B3 => tutorial_dynamic_pipeline().unwrap(),
        Tutorial::B4 => tutorial_queue().unwrap(),
        Tutorial::B5 => tutorial_guikit().unwrap(),
        Tutorial::B6 => tutorial_media_pad().unwrap(),
        Tutorial::B7 => tutorial_multithread_pad().unwrap(),
        Tutorial::B8 => tutorial_shortcut_pipeline().unwrap(),
        Tutorial::B9 { uri } => tutorial_media_info(&uri).unwrap(),
        Tutorial::B12 => tutorial_streaming().unwrap(),
        Tutorial::B13 => tutorial_playback_speed().unwrap(),
        Tutorial::T1 => preview_metadata().unwrap(),
    }
}
