/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* global Headers */

/**
 * This script has the logic to store and use the session token.
 *
 * It allows others to make requests authenticated with the stored
 * session token, if any.
 *
 * It also allows others to obtain information about this session token
 * (i.e. logged in user).
 */

'use strict';

(function(exports) {

  function sessionRequest(sessionInfo) {
    var auth = btoa(sessionInfo.username + ':' + sessionInfo.password);
    return fetch('/users/v1/login', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': 'Basic ' + auth
      }
    }).then(function(response) {
      return response.json();
    }).then(function(json) {
      if (!json || !json.session_token) {
        throw 'Unauthorized';
      }
      return json.session_token;
    });
  }

  var Session = {
    get: function() {
      if (!Session._token) {
        Session._token = localStorage.getItem('session');
      }
      return Session._token;
    },

    /**
     * Parse session token (JWT) to get the user email.
     */
    getUser: function() {
      if (Session._user) {
        return Session._user;
      }

      var token = Session.get();
      if (!token) {
        console.warn('Missing token');
        return;
      }

      // Session tokens has 3 parts: header, payload and signature.
      // The header contains information about the JWT itself.
      // The signature is used to verify that the JWT content is valid
      // and has not been modified. FoxBox's is responsible of doing
      // this check with every authenticated request.
      // The payload contains information about the session, including
      // the user id and email. This is the part that we care about in
      // this case.
      var parts = token.split('.');
      if (!parts || parts.length != 3) {
        console.warn('Invalid token');
        return;
      }
      var payload = parts[1];
      var output = payload.replace(/-/g, '+').replace(/_/g, '/');
      switch (output.length % 4) {
        case 0:
          break;
        case 2:
          output += '==';
          break;
        case 3:
          output += '=';
          break;
        default:
          throw 'Illegal base64url string!';
      }

      var result = window.atob(output);
      try{
        result = decodeURIComponent(escape(result));
      } catch (err) {
        console.warn(err);
      }

      try {
        Session._user = JSON.parse(result);
      } catch (err) {
        console.warn(err);
      }
      // Something like { id: 'uuid', email: 'a@b.com' }
      return Session._user;
    },

    /**
     * This method makes a POST request to the /users/v1/login
     * endpoint with the given username and password. If the request
     * succeeds a session token is returned in the response. If the
     * 'store' flag is true, we save this session token in memory and
     * in localStorage.
     */
    start: function(username, pwd, store) {
      if (!username || !pwd) {
        return Promise.reject();
      }
      return sessionRequest({
        username: username,
        password: pwd
      }).then(function(token) {
        if (store) {
          localStorage.setItem('session', token);
        }
        return token;
      });
    },

    clear: function() {
      localStorage.clear('session');
    },

    /**
     * This method allow others to make requests authenticated with
     * the stored session token to a given endpoint.
     */
    request: function(method, endpoint, body) {
      var options = {
        method: method,
        mode: 'cors',
        redirect: 'follow',
        headers: new Headers({
          'Authorization': 'Bearer ' + Session.get()
        })
      };

      if (body && body.length) {
        options.body = body;
      }

      return fetch(endpoint, options);
    }
  };

  exports.Session = Session;
}(window));
