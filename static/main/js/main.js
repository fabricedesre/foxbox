/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* global Console */
/* global getElementName */
/* global Session */
/* global URLSearchParams */
/* global Users */
/* global validateEmail */

/* exported Main */

/**
 * This script controls FoxBox's main UI.
 *
 * Currently, it shows two different screens (SIGN_IN and SIGNED_IN)
 * depending on whether a user session exists or not.
 *
 * If no user is logged in, we show a login screen.
 *
 * Otherwise, we show the admin panel containing a console to make requests
 * to FoxBox's HTTP API and a user management panel, where the admin can
 * invite new users and delete existing ones. The logic to control these two
 * sections (console and users) are in separated scripts (console.js and
 * users.js).
 */

'use strict';

var SIGN_IN         = 0;
var SIGNED_IN       = 1;

var ELEMENTS = [{
  screen: SIGN_IN,
  selector: '#signin-email'
}, {
  screen: SIGN_IN,
  selector: '#signin-pwd'
}, {
  screen: SIGN_IN,
  selector: '#signin-button',
  event: 'click',
  listener: 'signin'
}, {
  screen: SIGNED_IN,
  selector: '#signout-button',
  event: 'click',
  listener: 'signout'
}];

var Main = {
  get session() {
    if (!this._session) {
      this._session = Session.get();
    }
    return this._session;
  },

  init: function() {
    Main.elements = {};
    Main.screens = {
      signin: document.querySelector('#signin'),
      signedin: document.querySelector('#signedin')
    };

    if (Main.session === null) {
      // We might end up in the login screen because the user directly
      // browsed to FoxBox's URL or because the user was redirected from
      // an external app that is requesting access to FoxBox APIs.
      // In this case, we need to obtain the 'redirect_url' query parameter
      // so we can redirect the user back to the external app once the
      // login process is completed.
      var searchParams =
        new URLSearchParams(window.location.search.substring(1));

      if (searchParams.has('redirect_url')) {
        try {
          Main.redirect = new URL(searchParams.get('redirect_url'));
        } catch(e) {
          console.error(e);
        }
      }

      Main.show(SIGN_IN);
    } else if (Main.session.length) {
      Main.show(SIGNED_IN);
    }
  },

  addListener: function(element, event, listener) {
    if (!element || !event || !listener) {
      return;
    }
    element.addEventListener(event, this[listener]);
  },

  removeListener: function(element, event, listener) {
    if (!element || !event || !listener) {
      return;
    }
    element.removeEventListener(event, this[listener]);
  },

  /**
   * We try to load only the pieces of the DOM that are visible on the screen
   * depending on the session state.
   * Once a section is hidden, we remove all its event listeners.
   * This is happening for example when going from a logged out to a logged in
   * state and viceversa.
   */
  loadElements: function(screen) {
    var self = this;
    ELEMENTS.forEach(function(element) {
      var name = getElementName(element.selector);
      if (element.screen == screen) {
        try {
          self.elements[name] = document.querySelector(element.selector);
        } catch (e) {}
        if (element.event && element.listener) {
          self.addListener(self.elements[name],
                           element.event, element.listener);
        }
        return;
      }
      if (element.event && element.listener) {
        self.removeListener(self.elements[name],
                            element.event, element.listener);
      }
      self.elements[name] = null;
    });
  },

  /**
   * We show screens depending on the user session state.
   */
  show: function(screen) {
    if (this.currentScreen == screen) {
      return;
    }
    this.currentScreen = screen;
    this.screens.signin.hidden = (screen != SIGN_IN);
    this.screens.signedin.hidden = (screen != SIGNED_IN);

    if (screen == SIGNED_IN) {
      Console.setup();
      // XXX Do not show if user is not admin.
      Users.setup();
    } else {
      Console.teardown();
      Users.teardown();
    }

    this.loadElements(screen);
  },

  signin: function(evt) {
    evt.preventDefault();

    var email = Main.elements.signinEmail.value;
    if (!validateEmail(email)) {
      window.alert('Invalid email');
      return;
    }

    var pwd = Main.elements.signinPwd.value;
    if (!pwd || pwd.length < 8) {
      window.alert('Invalid password');
      return;
    }

    Session.start(email, pwd, Main.redirect === undefined).then(
      function(token) {
      if (Main.redirect) {
        var url = Main.redirect;
        url.search +=
         (url.search.split('?')[1] ? '&':'?') + 'session_token=' + token;
        url.hash = window.location.hash;
        window.location.replace(url.toString());
      } else {
        Main.show(SIGNED_IN);
      }
    }).catch(function(error) {
      window.alert('Signin error ' + error);
    });
  },

  signout: function() {
    Session.clear();
    Main.show(SIGN_IN);
  }
};

document.addEventListener('DOMContentLoaded', Main.init);
