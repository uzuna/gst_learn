# GstPluginTutorial

## Usage

```sh
export GST_PLUGIN_PATH=`pwd`/target/debug
gst-inspect-1.0 rstutorial
gst-launch-1.0 videotestsrc ! rsrgb2gray ! videoconvert ! autovideosink
```
