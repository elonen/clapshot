<script lang="ts">
  import { cur_username, cur_user_pic } from "../stores.js";
  import logo from "../assets/clapshot-logo.svg";
</script>

<nav
  class="bg-white border-gray-200 px-2 sm:px-4 py-2.5 rounded dark:bg-gray-900"
>
  <div class="container flex flex-wrap justify-between items-center mx-auto">
    <a href="/" class="flex items-center">
      <img
        src={logo}
        class="mr-3 h-6 sm:h-9 filter brightness-75"
        alt="Svelte"
      />
      <span
        class="self-center text-4xl whitespace-nowrap text-gray-300 align-text-bottom"
        style="font-family: 'Yanone Kaffeesatz', sans-serif;">CLAPSHOT</span
      >
    </a>
    <div class="flex items-center md:order-2">
      <h6 class="mx-4 text-gray-500 font-semibold">{$cur_username}</h6>
      <button
        type="button"
        class="flex mr-3 text-sm bg-gray-800 rounded-full md:mr-0 focus:ring-4 focus:ring-gray-300 dark:focus:ring-gray-600"
        id="user-menu-button"
        aria-expanded="false"
        data-dropdown-toggle="user-dropdown"
        data-dropdown-placement="bottom"
      >
        <span class="sr-only">Open user menu</span>
        <img
          src={$cur_user_pic}
          class="rounded-full p-2 w-12 h-12"
          alt=""
          loading="lazy"
        />
      </button>
      <!-- Dropdown menu -->
      <div
        class="hidden z-50 my-4 text-base list-none bg-white rounded divide-y divide-gray-100 shadow dark:bg-gray-700 dark:divide-gray-600"
        id="user-dropdown"
        style="position: absolute; inset: 0px auto auto 0px; margin: 0px; transform: translate(0px, 10px);"
        data-popper-reference-hidden=""
        data-popper-escaped=""
        data-popper-placement="bottom"
      >
        <div class="py-3 px-4">
          <span class="block text-sm text-gray-900 dark:text-white"
            >Bonnie Green</span
          >
          <span
            class="block text-sm font-medium text-gray-500 truncate dark:text-gray-400"
            >name@flowbite.com</span
          >
        </div>
        <ul class="py-1" aria-labelledby="user-menu-button">
          <li>
            <a
              href="#top"
              class="block py-2 px-4 text-sm text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:text-gray-200 dark:hover:text-white"
              >Dashboard</a
            >
          </li>
          <li>
            <a
              href="#top"
              class="block py-2 px-4 text-sm text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:text-gray-200 dark:hover:text-white"
              >Settings</a
            >
          </li>
          <li>
            <a
              href="#top"
              class="block py-2 px-4 text-sm text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:text-gray-200 dark:hover:text-white"
              >Sign out</a
            >
          </li>
        </ul>
      </div>
      <button
        data-collapse-toggle="mobile-menu-2"
        type="button"
        class="inline-flex items-center p-2 ml-1 text-sm text-gray-500 rounded-lg md:hidden hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-gray-200 dark:text-gray-400 dark:hover:bg-gray-700 dark:focus:ring-gray-600"
        aria-controls="mobile-menu-2"
        aria-expanded="false"
      >
        <span class="sr-only">Open main menu</span>
        <svg
          class="w-6 h-6"
          aria-hidden="true"
          fill="currentColor"
          viewBox="0 0 20 20"
          xmlns="http://www.w3.org/2000/svg"
          ><path
            fill-rule="evenodd"
            d="M3 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM3 10a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM3 15a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z"
            clip-rule="evenodd"
          /></svg
        >
      </button>
    </div>
    <div
      class="hidden justify-between items-center w-full md:flex md:w-auto md:order-1"
      id="mobile-menu-2"
    >
      <ul
        class="flex flex-col p-4 mt-4 bg-gray-50 rounded-lg border border-gray-100 md:flex-row md:space-x-8 md:mt-0 md:text-sm md:font-medium md:border-0 md:bg-white dark:bg-gray-800 md:dark:bg-gray-900 dark:border-gray-700"
      >
        <li>
          <a
            href="#top"
            class="block py-2 pr-4 pl-3 text-gray-700 rounded hover:bg-gray-100 md:hover:bg-transparent md:hover:text-blue-700 md:p-0 dark:text-gray-400 md:dark:hover:text-white dark:hover:bg-gray-700 dark:hover:text-white md:dark:hover:bg-transparent dark:border-gray-700"
            >Browse</a
          >
        </li>
        <li>
          <a
            href="#top"
            class="block py-2 pr-4 pl-3 text-gray-700 rounded hover:bg-gray-100 md:hover:bg-transparent md:hover:text-blue-700 md:p-0 dark:text-gray-400 md:dark:hover:text-white dark:hover:bg-gray-700 dark:hover:text-white md:dark:hover:bg-transparent dark:border-gray-700"
            >Upload</a
          >
        </li>
        <li>
          <a
            href="#top"
            class="block py-2 pr-4 pl-3 text-gray-700 rounded hover:bg-gray-100 md:hover:bg-transparent md:hover:text-blue-700 md:p-0 dark:text-gray-400 md:dark:hover:text-white dark:hover:bg-gray-700 dark:hover:text-white md:dark:hover:bg-transparent dark:border-gray-700"
            >About</a
          >
        </li>
      </ul>
    </div>
  </div>
</nav>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Roboto+Condensed&family=Yanone+Kaffeesatz&display=swap");
</style>
