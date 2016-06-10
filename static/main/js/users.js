/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* global getElementName */
/* global Session */
/* global validateEmail */

/* exported Users */

/**
 * This script controls the user management section from the main screen,
 * where the admin user can invite new users and delete existing ones.
 */
'use strict';

var Users = {
  setup: function() {
    ['#user',
     '#users-list',
     '#users-invite-email',
     '#users-invite-button'].forEach(function(selector) {
      var name = getElementName(selector);
      this[name] = document.querySelector(selector);
    }.bind(this));

    this._invite = this.invite.bind(this);

    this.usersInviteButton.addEventListener('click', this._invite);

    this.getUsers();
  },

  teardown: function() {
    if (Users.usersInviteButton) {
      Users.usersInviteButton.removeEventListener('click', Users._invite);
    }

    var list = Users.usersList;
    if (!list) {
      return;
    }

    while (list.firstChild) {
      list.removeChild(list.firstChild);
    }
  },

  /**
   * Get the entire list of registered users and append them to the DOM
   */
  getUsers: function() {
    // Set the current logged in user.
    Users.user.textContent = Session.getUser().email;
    // Obtain the list of registered users
    Session.request('GET', '/users/v1/users').then(function(response) {
      return response.json();
    }).then(function(json) {
      if (!json.users) {
        return json.message || 'Could not retrieve user list';
      }
      // Clean user list.
      var list = Users.usersList;
      while (list.firstChild) {
        list.removeChild(list.firstChild);
      }
      // For each user, append a new element to the DOM containing the user
      // email and a button to delete it. Inactive users will be shown in gray.
      json.users.forEach(Users.addUser);
    }).catch(function(error) {
      alert(error);
    });
  },

  /**
   * Invites a user given its email.
   *
   * The user is immediately added to the DOM.
   */
  invite: function() {
    var email = Users.usersInviteEmail.value;

    if (!email) {
      alert('Missing email');
      return;
    }

    if (!validateEmail(email)) {
      alert('Invalid email');
      return;
    }

    Session.request('POST', '/users/v1/users', JSON.stringify({
      email: email
    })).then(function(response) {
      if (response.status != 204) {
        response.json().then(function(error) {
          throw error.message || 'Could not send invitation';
        });
      }
      Users.usersInviteEmail.value = '';
      // Refresh user list.
      Users.getUsers();
    }).catch(function(error) {
      alert(error);
    });
  },

  /**
   * Append a user entry to the DOM showing its email and a button to
   * delete it.
   */
  addUser: function(user) {
    var row = document.createElement('tr');

    var email = document.createElement('td');
    email.textContent = user.email;
    row.appendChild(email);

    var remove = document.createElement('td');
    var removeButton = document.createElement('button');
    removeButton.textContent = 'Delete';
    if (!Users._removeListeners) {
      Users._removeListeners = {};
    }
    var id = user.id;
    Users._removeListeners[id] = function() {
      Users.removeUser(id, removeButton);
    };
    removeButton.addEventListener('click', Users._removeListeners[id]);
    remove.appendChild(removeButton);
    row.appendChild(remove);

    Users.usersList.appendChild(row);
  },

  /**
   * Method to try to remove a user from the DB.
   *
   * If the delete request succeeds, the user is removed from the DOM.
   */
  removeUser: function(id, button) {
    Session.request('DELETE', '/users/v1/users/' + id)
    .then(function(response) {
      if (response.status != 204) {
        return response.json().then(function(error) {
          throw error.message || 'Could not delete user';
        });
      }
      button.removeEventListener('click', Users._removeListeners[id]);
      var row = button.parentElement.parentElement;
      Users.usersList.removeChild(row);
      delete Users._removeListeners[id];
    }).catch(function(error) {
      alert(error);
    });
  }
};
