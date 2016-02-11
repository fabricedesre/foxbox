/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

function success() {
  document.getElementById('location-span').innerHTML = window.location.href;
  document.getElementById('set-admin-pwd').style.display = 'none';
  document.getElementById('success-message').style.display = 'block';
}

function setPassword() {
  var pwd = document.getElementById('pwd1-input').value;
  if (document.getElementById('pwd2-input').value != pwd) {
    window.alert('Passwords don\'t match! Please try again.');
    return;
  }
  if (pwd.length < 8) {
    window.alert('Please use a password of at least 8 characters.');
    return;
  }
  var xhr = new XMLHttpRequest();
  xhr.open('POST', '/setup', true);
  xhr.onload = function() {
    if (xhr.status != 204) {
      console.log('TO DO: Deal with unsuccessful API response', xhr.status,
          xhr.responseText);
    }
    // Note: If successful, the API call will have returned a session token,
    // but we currently don't use this for anything.
    // TODO: Save this session token on the client-side (e.g. in IndexedDB)
    // for reuse in the OAuth dialog UI, so that the user does not have to
    // retype the password they just created when connecting an app via OAuth
    // the first time.
    success();
  };
  // See https://github.com/fxbox/users/blob/master/doc/API.md#post-setup
  xhr.setRequestHeader('Content-Type', 'application/json');
  xhr.send(JSON.stringify({
    email: 'admin@foxbox.local',
    username: 'admin',
    password: pwd
  }));
}

document.getElementById('set-button').onclick =  setPassword;
