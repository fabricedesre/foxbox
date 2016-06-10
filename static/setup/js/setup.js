/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* global URLSearchParams */
/* global validateEmail */

/**
 * This script has the logic to manage the FTU where the user is asked
 * to set up his admin account. To set up an admin account a email and
 * a password are required.
 *
 * This code is only served by foxbox when no admin user is found in the
 * users db.
 */

'use strict';

var SetupUI = {
  init: function() {
    SetupUI.elements = {
      signupEmail: document.querySelector('#signup-email'),
      signupPwd1: document.querySelector('#signup-pwd1'),
      signupPwd2: document.querySelector('#signup-pwd2'),
      signupButton: document.querySelector('#signup-button'),
    };
    SetupUI.screens = {
      signup: document.querySelector('#signup'),
      signupSuccess: document.querySelector('#signup-success')
    };

    var searchParams =
      new URLSearchParams(window.location.search.substring(1));

    if (searchParams.has('redirect_url')) {
      try {
        SetupUI.redirect = new URL(searchParams.get('redirect_url'));
      } catch(e) {
        console.error(e);
      }
    }

    SetupUI.elements.signupButton.addEventListener('click', SetupUI.signup);
  },

  /**
   * Make the request to foxbox's users HTTP API to create the admin user
   * based on the user input (email and password).
   *
   * If the request succeeds and the url contains a 'redirect_url' query
   * parameter, we redirect the user to that url with the session token
   * also as query parameter. Otherwise, we show a success screen.
   */
  signup: function(evt) {
    evt.preventDefault();

    var email = SetupUI.elements.signupEmail.value;
    if (!validateEmail(email)) {
      window.alert('Invalid email');
      return;
    }

    var pwd = SetupUI.elements.signupPwd1.value;
    if (pwd != SetupUI.elements.signupPwd2.value) {
      window.alert('Passwords don\'t match! Please try again.');
      return;
    }

    if (pwd.length < 8) {
      window.alert('Please use a password of at least 8 characters.');
      return;
    }

    fetch('/users/v1/setup', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        name: 'admin',
        email: email,
        password: pwd
      })
    }).then(function(response) {
      if (response.status != 201) {
        return response.json().then(function(error) {
          throw error.message || 'Invalid response';
        });
      }
      return response.json();
    }).then(function(json) {
      var token = json.session_token;
      if (!token) {
        throw 'Missing session token';
      }

      if (SetupUI.redirect) {
        var url = SetupUI.redirect;
        url.search +=
         (url.search.split('?')[1] ? '&':'?') + 'session_token=' + token;
        url.hash = window.location.hash;
        window.location.replace(url.toString());
      } else {
        localStorage.setItem('session', token);
        SetupUI.screens.signupSuccess.hidden = false;
        SetupUI.screens.signup.hidden = true;
      }
    }).catch(function(error) {
      alert(error);
    });
  }
};

document.addEventListener('DOMContentLoaded', SetupUI.init);
