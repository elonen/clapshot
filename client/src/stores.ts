import { writable, type Writable } from 'svelte/store';
import type * as Proto3 from '../../protobuf/libs/typescript';

export let video_url = writable(null);
export let video_hash = writable(null);
export let video_fps = writable(42);
export let video_title = writable("(no video loaded)");
export let video_progress_msg = writable(null);

export let cur_page_items: Writable<Proto3.PageItem[]> = writable([]);

export let cur_username = writable(null);
export let cur_user_id = writable(null);
export let cur_user_pic = writable(null);

export let video_is_ready = writable(false);

export let all_comments = writable([]);
export let user_messages = writable([]);

export let collab_id = writable(null);

export let user_menu_items = writable([]);

export let selected_tiles = writable([]);
