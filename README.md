# qdx-receiver

## What is this?
A small program to feed the audio from your QRP-Labs QDX digital transceiver through to your computer's speakers. Also
gives you a small user interface in which you can see a signal-strength meter, control the output volume coming from
your speakers, and to tune the QDX to a particular frequency.

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

## Project Status
Project started 9 Jul 2023, currently very rough, not fully working.

# License, Copyright & Contact info
This code is released under the Apache 2.0 License: http://www.apache.org/licenses/LICENSE-2.0.html.

(C) 2023 Matt J. Gumbley

matt.gumbley@devzendo.org

Mastodon: @M0CUV@mastodon.radio

http://devzendo.github.io/qdx-receiver

