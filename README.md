# qdx-receiver

## What is this?
A small program to feed the audio from your QRP-Labs QDX digital transceiver through to your computer's speakers. Also
gives you a small user interface in which you can see a signal-strength meter, control the output volume coming from
your speakers, and to tune the QDX to a particular frequency (receive SSB and other signals!).

## OK, Why?
The QDX has the following characteristics:
* High performance embedded SDR SSB receiver with 60-70dB of unwanted sideband cancellation
* Built-in 24-bit 48ksps USB sound card
* Built-in USB Virtual COM Serial port for CAT control
* Si5351A Synthesized VFO with 25MHz TCXO as standard
So it's a superb receiver, and works very well with software such as WSJT-X etc. for digital amateur radio modes.

However, its RF amplifier expects to see a well-matched antenna; if the antenna is presenting a poor match to the
transmitter, it's quite likely that you will blow the BS170 power amplifier transistors. So, how are you to tune the
antenna (using an ATU) without transmitting? Typically this can be done by listening to the received audio of the
radio, whilst tuning the antenna, listening for the loudest signal. You're then in the right area, and transmission
can be used to fine-tune.

Except that the QDX has no speaker. You could watch the received audio meter in WSJT-X, but I find it easier to
tune to an empty part of the band, and assess the noise with my ears.

That's where qdx-receiver could help.

Plus you could use it to tune around and find other interesting signals!

## Project Status
Project started 9 Jul 2023, currently a bit rough, but working.

Known issues:
* Some audio artifacts, especially soon after startup. Probably my gain setting is wrong.
* The signal strength meter is "an attempt" - it vaguely matches my real transceiver's meter, sometimes.

Needed before a proper release:
* No packaging/release system yet. Build it yourself from source.
* Finds the QDX audio output device, but only finds the speaker device as named on a MacBook or my old Windows laptop. (Eventually you'll be able to choose this.)
* Developed and tested best on macOS Catalina. Should build on Linux and Windows.

Nice to have:
* Volume control could adjust the QDX's actual gain.
* No configuration yet (to store the last tuned frequency on each band; the last used frequency; the amplitude).
* Make the GUI "beautiful".

Really nice to have:
* A waterfall display.
* Bandpass/notch filtering for CW.
* DSP noise reduction.
* The moon on a stick.

## How To Use qdx-receiver
* Use the banks of up/down arrow buttons to increase/decrease a digit in the frequency display.
* Use your mouse wheel to continue incrementing/decrementing that digit, to scroll through the spectrum.
* Click one of the "metre bands" buttons to switch to the FT8 frequency of that band.
* Adjust the output volume with the volume slider.
* Toggle the audio on/off with the mute button.

## Building
There's no download yet, so to try it you'll need the Rust toolchain. See https://rustup.rs for how to obtain it.
With modern Rust installed, clone this repo.
You'll also need to install portaudio and fltk. No docs on how to do this yet.
From the command line, cd to your cloned repo, and do:
`cargo build`
Now make a nice cup of tea.
To run, do:
`cargo run`

# Notes
* CAT spec at https://www.kenwood.com/i/products/info/amateur/ts_480/pdf/ts_480_pc.pdf
* Meter scale from https://www.pi4frg.nl/857meter_eng.htm

# License, Copyright & Contact info
This code is released under the Apache 2.0 License: http://www.apache.org/licenses/LICENSE-2.0.html.

(C) 2023 Matt J. Gumbley

matt.gumbley@devzendo.org

Mastodon: @M0CUV@mastodon.radio

http://devzendo.github.io/qdx-receiver

