/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

var socket = new WebSocket("ws://localhost:4000/services");

socket.onmessage = function(event) {
  var body = document.body;
  body.appendChild(document.createElement("br"));
  body.appendChild(document.createTextNode(event.data));
};

socket.onclose = function(event) {
  var body = document.body;
  body.appendChild(document.createElement("br"));
  body.innerHTML += "<pre>services WebSocket closed</pre>";
}
