extern crate gstreamer as gst;
use std::{ffi::c_void, io::Write};

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
    gst::init().unwrap();

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
        B5,
        B6,
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
        Tutorial::B5 => tutorial_guikit().unwrap(),
        Tutorial::B6 => tutorial_media_pad().unwrap(),
    }
}
