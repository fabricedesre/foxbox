/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

'use strict';

const assert = require('assert');
const SuiteBuilder = require('./lib/make_suite');
const PASSWORDS = require('./lib/passwords.json');

var suiteBuilder = new SuiteBuilder('Test set up UI');

suiteBuilder.build((setUpWebapp) => {

describe('sessions ui', function() {

  describe('Foxbox index', function() {

    describe('signup', function() {
      var setUpView;

      beforeEach(function() {
        return suiteBuilder.browserCleanUp().then(() => {
          setUpView = setUpWebapp.setUpView;
        });
      });

      describe('failures', function() {

        afterEach(function() {
          return setUpView.dismissAlert();
        });

        PASSWORDS.invalids.forEach((invalidPassword) => {
          it('should reject ' + invalidPassword.reason + ' passwords',
          function() {
            return setUpView
              .failureLogin(invalidPassword.value, invalidPassword.value)
              .then(text => {
                assert.equal(text,
                  'Please use a password of at least 8 characters.');
              });
          });
        });

        it('should reject non-matching passwords', function() {
          return setUpView.failureLogin(12345678, 1234).then(text => {
            assert.equal(text, 'Passwords don\'t match! Please try again.');
          });
        });
      });

      describe('success', function() {

        after(function() {
          return suiteBuilder.restartFromScratch();
        });

        it('should accept matching, long-enough passwords', function() {
          return setUpView.successLogin()
            .then(successfulPageView => successfulPageView.loginMessage)
            .then(text => { assert.equal(text, 'Thank you!'); });
        });
      });
    });
  });

  describe('once registred', function() {
    var signedInView;

    before(function() {
      return setUpWebapp.init()
        .then(setUpView => setUpView.successLogin())
        .then(successfulView => successfulView.goToSignedIn())
        .then(view => { signedInView = view; });
    });

    describe('signedin page', function() {
      it('should sign out', function() {
        return signedInView.signOut();
      });
    });

    describe('signin page', function() {
      var signInView;

      beforeEach(function() {
        return suiteBuilder.browserCleanUp()
          .then(() => { signInView = setUpWebapp.signInPage; });
      });

      PASSWORDS.invalids.forEach(invalidPassword => {
        it('should reject ' + invalidPassword.reason + ' passwords',
        function() {
          return signInView.failureLogin(invalidPassword.value)
            .then(alertMessage => {
              assert.equal(alertMessage, 'Invalid password');
            });
        });
      });

      it('should reject not matching passwords', function() {
        return signInView.failureLogin('longEnoughButInvalid')
          .then(alertMessage => {
            assert.equal(alertMessage, 'Signin error Unauthorized');
          });
      });

      it('should accept matching, long-enough passwords', function() {
        return signInView.successLogin();
      });
    });
  });
});
});
