import { writable, type Writable } from 'svelte/store';
import type * as Proto3 from '../../protobuf/libs/typescript';
import type { IndentedComment, UserMenuItem } from './types';
import type { VideoListDefItem } from './lib/video_list/types';

export let video_url: Writable<string|null> = writable(null);
export let video_hash: Writable<string|null> = writable(null);
export let video_fps: Writable<number|null> = writable(null);
export let video_title: Writable<string|null> = writable("(no video loaded)");
export let video_progress_msg: Writable<string|null> = writable(null);

export let cur_page_items: Writable<Proto3.PageItem[]> = writable([]);

export let cur_username: Writable<string|null> = writable(null);
export let cur_user_id: Writable<string|null> = writable(null);
export let cur_user_pic: Writable<string|null> = writable(null);

export let video_is_ready: Writable<boolean> = writable(false);

export let all_comments: Writable<IndentedComment[]> = writable([]);
export let user_messages: Writable<Proto3.UserMessage[]> = writable([]);

export let collab_id: Writable<string|null> = writable(null);
export let user_menu_items: Writable<UserMenuItem[]> = writable([]);
export let selected_tiles: Writable<{[key: string]: VideoListDefItem}> = writable({});
export let server_defined_actions: Writable<{ [key: string]: Proto3.ActionDef }> = writable({});
