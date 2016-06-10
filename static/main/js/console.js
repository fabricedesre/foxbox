/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* global getElementName */
/* global Session */

/* exported Console */

'use strict';

var Console = {
  setup: function() {
    ['#console-method',
     '#console-endpoint',
     '#console-body',
     '#console-send',
     '#console-clear',
     '#console-response',
     '#console-response-content'].forEach(function(selector) {
      var name = getElementName(selector);
      this[name] = document.querySelector(selector);
    }.bind(this));

    this._send = this.send.bind(this);
    this._clear = this.clear.bind(this);

    this.consoleSend.addEventListener('click', this._send);
    this.consoleClear.addEventListener('click', this._clear);
  },

  teardown: function() {
    if (this.consoleSend) {
      this.consoleSend.removeEventListener('click', this._send);
    }
    if (this.consoleClear) {
      this.consoleClear.removeEventListener('click', this._clear);
    }
  },

  showResponse: function(content) {
    this.consoleResponse.hidden = false;
    var element = this.consoleResponseContent;
    element.textContent = content;
    element.style.height = element.scrollHeight + 'px';
  },

  send: function() {
    var endpoint = this.consoleEndpoint.value;
    var method = this.consoleMethod.value;
    var body = this.consoleBody.value;

    if (!endpoint || !method) {
      this.showResponse('Missing endpoint or method');
      return;
    }

    var self = this;
    var responseText = '';
    Session.request(method, endpoint, body).then(function(response) {
      responseText += response.url + '\n' +
                      response.status + ' ' + response.statusText + '\n';
      var headerKeys = response.headers.keys();
      var header;
      while ((header = headerKeys.next())) {
        if (header.done) {
          break;
        }
        responseText += header.value + ': ' +
          response.headers.get(header.value) + '\n';
      }
      return response.text();
    }).then(function(text) {
      if (text) {
        responseText += '\n\n' + text;
      }
      self.showResponse(responseText);
    }).catch(function(error) {
      console.error('ERROR', error);
      self.showResponse(responseText);
    });
  },

  clear: function() {
    this.consoleEndpoint.value = '';
    this.consoleBody.value = '';
    this.consoleResponseContent = '';
    this.consoleResponse.hidden = true;
  }
};
