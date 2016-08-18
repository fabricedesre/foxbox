/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

this.addEventListener('install', function(event) {
  debug("Installed.");
});

this.addEventListener('fetch', function(event) {
  console.log(`Fetching ${event.request.url}.`);
  let response = new Response('<p>Hello from your friendly neighbourhood service worker!</p>', {
    headers: { 'Content-Type': 'text/html' }
  })
  event.respondWith(Promise.resolve(response));
});
