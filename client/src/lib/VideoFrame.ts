// Video framerate and timecode calculation utils.
//
// Based on work by Allen Sarkisyan, modified for TypeScript and customized
// for Clapshot by Jarno Elonen.
//
// Original license was MIT, modified version is GPL (as the rest of Clapshot).
// See both licenses below.

// *TODO*: There's quite a bit of unnecessary code for Clapshot's purposes
// in this file, probably unintentionally introduced bugs too. Clean up
// and simplify.

/*
 * Copyright (c) 2023  Jarno Elonen   
 *  
 *  This file is free software: you may copy, redistribute and/or modify it  
 *  under the terms of the GNU General Public License as published by the  
 *  Free Software Foundation, either version 2 of the License, or (at your  
 *  option) any later version.  
 *  
 *  This file is distributed in the hope that it will be useful, but  
 *  WITHOUT ANY WARRANTY; without even the implied warranty of  
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU  
 *  General Public License for more details.  
 *  
 *  You should have received a copy of the GNU General Public License  
 *  along with this program.  If not, see .  
 *  
 * This file incorporates work covered by the following copyright and  
 * permission notice:  
 *  
 *     Copyright (c) 2013, Allen Sarkisyan   
 *  
 *     Permission to use, copy, modify, and/or distribute this software  
 *     for any purpose with or without fee is hereby granted, provided  
 *     that the above copyright notice and this permission notice appear  
 *     in all copies.  
 *  
 *     THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL  
 *     WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED  
 *     WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE  
 *     AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR  
 *     CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS  
 *     OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,  
 *     NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN  
 *     CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.  
 */

/* @preserve
VideoFrame: HTML5 Video - SMTPE Time Code capturing and Frame Seeking API
@version 0.2.2
@author Allen Sarkisyan
@copyright (c) 2013 Allen Sarkisyan 
@license Released under the Open Source MIT License

Contributors:
Allen Sarkisyan - Lead engineer
Paige Raynes - Product Development
Dan Jacinto - Video Asset Quality Analyst

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, and/or distribute copies of the
Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

- The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
- Attribution must be credited to the original authors in derivative works.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

export class VideoFrame
{
	video: HTMLVideoElement;
	obj: any;
	frameRate: number;
	interval: any;
	fps: number;

	constructor(options: any) {
		this.obj = options || {};
		this.frameRate = this.obj.frameRate || 24;
		this.video = this.obj.video || document.getElementById(this.obj.id) || document.createElement('video');
	}

	/**
	 * Returns the current frame number
	 * 
	 * @return {Number} - Frame number in video
	 */
	get(): number {
		return Math.floor(this.video.currentTime * this.frameRate);
	}

	/**
	 * Event listener for handling callback execution at double the current frame rate interval
	 * 
	 * @param  {String} format - Accepted formats are: SMPTE, time, frame
	 * @param  {Number} tick - Number to set the interval by.
	 * @return {Number} Returns a value at a set interval
	 */
	listen(format: string, tick: number): number {
		if (!format) { console.log('VideoFrame: Error - The listen method requires the format parameter.'); return; }
		this.interval = setInterval(function() {
			if (this.video.paused || this.video.ended) { return; }
			var frame = ((format === 'SMPTE') ? this.toSMPTE() : ((format === 'time') ? this.toTime() : this.get()));
			if (this.obj.callback) { this.obj.callback(frame, format); }
			return frame;
		}, (tick ? tick : 1000 / this.frameRate / 2));
	}

	/** Clears the current callback interval */
	stopListen(): void {
		clearInterval(this.interval);
	}

	/**
	 * Returns the current time code in the video in HH:MM:SS format
	 * - used internally for conversion to SMPTE format.
	 * 
	 * @param  {Number} frames - The current time in the video
	 * @return {String} Returns the time code in the video
	 */
	toTime(frames: number): string {
		var time = (typeof frames !== 'number' ? this.video.currentTime : frames), frameRate = this.frameRate;
		var dt = (new Date()), format = 'hh:mm:ss' + (typeof frames === 'number' ? ':ff' : '');
		dt.setHours(0); dt.setMinutes(0); dt.setSeconds(0); dt.setMilliseconds(time * 1000);
		function pad(n: number) { return n.toFixed(0).padStart(2, "0"); }
		return format.replace(/hh|mm|ss|ff/g, function(format: string): string {
			switch (format) {
				case "hh": return pad(dt.getHours() < 13 ? dt.getHours() : (dt.getHours() - 12));
				case "mm": return pad(dt.getMinutes());
				case "ss": return pad(dt.getSeconds());
				case "ff": return pad(Math.floor(((time % 1) * frameRate)));
			}
		});
	}

	/**
	 * Returns the current SMPTE Time code in the video.
	 * - Can be used as a conversion utility.
	 * 
	 * @param  frame - OPTIONAL: Frame number for conversion to it's equivalent SMPTE Time code.
	 * @return Returns a SMPTE Time code in HH:MM:SS:FF format
	 */
	toSMPTE(frame?: number): string {
		if (typeof(frame) == 'undefined') { return this.toTime(this.video.currentTime); }
		let frameNumber = Number(frame);
		let fps = this.frameRate;
		const MIN = (fps * 60);
		const HOUR = (MIN * 60);
		let hours = frameNumber / HOUR;
		let minutes = (Math.floor(frameNumber / MIN) % 60);
		let seconds = (Math.floor(frameNumber / fps) % 60);
		function pad(n: number) { return n.toFixed(0).padStart(2, "0"); }
		return (pad(hours) + ':' + pad(minutes) + ':' + pad(seconds) + ':' + pad(Math.round(frameNumber % fps)));
	}

	/**
	 * Converts a SMPTE Time code to Seconds
	 * 
	 * @param  {String} SMPTE - a SMPTE time code in HH:MM:SS:FF format
	 * @return {Number} Returns the Second count of a SMPTE Time code
	 */
	toSeconds(SMPTE: string): number {
		if (!SMPTE) { return Math.floor(this.video.currentTime); }
		var time = SMPTE.split(':');
		return (((Number(time[0]) * 60) * 60) + (Number(time[1]) * 60) + Number(time[2]));
	}

	/**
	 * Converts a SMPTE Time code, or standard time code to Milliseconds
	 * 
	 * @param  {String} SMPTE OPTIONAL: a SMPTE time code in HH:MM:SS:FF format,
	 * or standard time code in HH:MM:SS format.
	 * @return {Number} Returns the Millisecond count of a SMPTE Time code
	 */
	toMilliseconds(SMPTE: string): number {
		var frames = (!SMPTE) ? Number(this.toSMPTE().split(':')[3]) : Number(SMPTE.split(':')[3]);
		var milliseconds = (1000 / this.frameRate) * (isNaN(frames) ? 0 : frames);
		return Math.floor(((this.toSeconds(SMPTE) * 1000) + milliseconds));
	}

	/**
	 * Converts a SMPTE Time code to it's equivalent frame number
	 * 
	 * @param  {String} SMPTE - OPTIONAL: a SMPTE time code in HH:MM:SS:FF format
	 * @return {Number} Returns the long running video frame number
	 */
	toFrames(SMPTE: string): number {
		var time = (!SMPTE) ? this.toSMPTE().split(':') : SMPTE.split(':');
		var frameRate = this.frameRate;
		var hh = (((Number(time[0]) * 60) * 60) * frameRate);
		var mm = ((Number(time[1]) * 60) * frameRate);
		var ss = (Number(time[2]) * frameRate);
		var ff = Number(time[3]);
		return Math.floor((hh + mm + ss + ff));
	}

	/**
	 * Private - seek method used internally for the seeking functionality.
	 * 
	 * @param  {String} direction - Accepted Values are: forward, backward
	 * @param  {Number} frames - Number of frames to seek by.
	 */
	__seek(direction: string, frames: number): void {
		if (!this.video.paused) { this.video.pause(); }
		var frame = Number(this.get());
		/** To seek forward in the video, we must add 0.00001 to the video runtime for proper interactivity */
		this.video.currentTime = ((((direction === 'backward' ? (frame - frames) : (frame + frames))) / this.frameRate) + 0.00001);
	}

	/**
	 * Seeks forward [X] amount of frames in the video.
	 * 
	 * @param  {Number} frames - Number of frames to seek by.
	 * @param  {Function} callback - Callback function to execute once seeking is complete.
	 */
	seekForward(frames: number, callback: Function): boolean {
		if (!frames) { frames = 1; }
		this.__seek('forward', Number(frames));
		return (callback ? callback() : true);
	}

	/**
	 * Seeks backward [X] amount of frames in the video.
	 * 
	 * @param  {Number} frames - Number of frames to seek by.
	 * @param  {Function} callback - Callback function to execute once seeking is complete.
	 */
	seekBackward(frames: number, callback: Function): boolean {
		if (!frames) { frames = 1; }
		this.__seek('backward', Number(frames));
		return (callback ? callback() : true);
	}

	/**
	 * For seeking to a certain SMPTE time code, standard time code, frame, second, or millisecond in the video.
	 * - Was previously deemed not feasible. Veni, vidi, vici.
	 *  
	 * example: { SMPTE: '00:01:12:22' }, { time: '00:01:12' },  { frame: 1750 }, { seconds: 72 }, { milliseconds: 72916 }
	 */
	seekTo(config: {}): void {
		var obj = config || {}, seekTime: number, smpte: string;
		/** Only allow one option to be passed */
		var option = Object.keys(obj)[0];

		if (option == 'SMPTE' || option == 'time') {
			smpte = obj[option];
			seekTime = ((this.toMilliseconds(smpte) / 1000) + 0.001);
			this.video.currentTime = seekTime;
			return;
		}

		switch(option) {
			case 'frame':
				smpte = this.toSMPTE(obj[option]);
				seekTime = ((this.toMilliseconds(smpte) / 1000) + 0.001);
				break;
			case 'seconds':
				seekTime = Number(obj[option]);
				break;
			case 'milliseconds':
				seekTime = ((Number(obj[option]) / 1000) + 0.001);
				break;
		}
		
		if (!isNaN(seekTime)) {
			this.video.currentTime = seekTime;
		}
	}
};
