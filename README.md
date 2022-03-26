
with X11 GTK `cargo run --features tutorial5-x11 -- b5`

## Reference

### Element

#### GstBaseSrc

##### Property

- blocksize: チャンクサイズ
- do-timestamp: タイムスタンプの付与
- num-buffers: 送信するチャンク数。送信したら終了する
- typefind: ?

#### GstBaseSink

##### Property

- async: 疎なストリームや同期が必要ないものにはtrueをするとよい?
- blocksize: pull時にsinkに渡すデータのチャンクサイズ
- enable-last-sample: 最後のバッファへの参照を保持するかどうか。早くバッファを開放したい場合はfalseにする
- last-sample: 最後に受信したバッファ
- max-bitrate: 1秒あたりのデータ転送量制限
- max-lateness: ?
- processing-deadline: パイプラインがバッファ処理に使える最大時間
- qos: ?
- render-delay: メディア同期とレンダリングの間の遅延。他のシンクの遅延補正のために追加の遅延を与える場合に設定する
- stats: 統計値
- throttle-time: ?
- ts-offset: 最終的な同期制御の補正
