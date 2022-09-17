import { writable } from 'svelte/store';

export let video_url = writable(null);
export let video_hash = writable(null);
export let video_fps = writable(42);
export let video_orig_filename = writable("(no video loaded)");

export let all_my_videos = writable([]);

export let cur_username = writable(null);
export let cur_user_id = writable(null);
export let cur_user_pic = writable(null);

export let video_is_ready = writable(false);

export let all_comments = writable([]);
