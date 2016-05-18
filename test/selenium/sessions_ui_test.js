/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

'use strict';

const assert= require('assert');
const SuiteBuilder = require('./lib/make_suite');

var suiteBuilder = new SuiteBuilder('Test set up UI');

suiteBuilder.build((setUpWebapp) => {

describe('sessions ui', function() {

  describe('Foxbox index', function() {

    describe('signup', function() {
      var setUpView;

      beforeEach(function() {
        return suiteBuilder.browserCleanUp().then(() => {
          setUpView = setUpWebapp.getSetUpView();
        });
      });

      it('should show the signup screen by default', function() {
        return setUpView.isSetUpView();
      });

      describe('failures', function() {

        const SHORT_PASSWORD_ERROR_MESSAGE =
          'Please use a password of at least 8 characters.';

        afterEach(function() {
          return setUpView.dismissAlert();
        });

        it('should reject non-matching passwords', function() {
          return setUpView.failureLogin(12345678, 1234)
          .then(text => {
            assert.equal(text, 'Passwords don\'t match! Please try again.');
          });
        });

        it('should reject short passwords', function () {
          return setUpView.failureLogin(1234, 1234)
          .then(text => { assert.equal(text, SHORT_PASSWORD_ERROR_MESSAGE); });
        });

        it('should fail if password is not set', function() {
          return setUpView.failureLogin('', '')
          .then(text => { assert.equal(text, SHORT_PASSWORD_ERROR_MESSAGE); });
        });
      });

      describe('success', function() {

        after(() => suiteBuilder.restartFromScratch());

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

    before(() => {
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
        .then(() => { signInView = setUpWebapp.getSignInPage(); });
      });

      [{
        test: 'should reject short passwords',
        pass: 'short',
        error: 'Invalid password'
      }, {
        test: 'should reject not matching passwords',
        pass: 'longEnoughButInvalid',
        error: 'Signin error Unauthorized'
      }, {
        test: 'should fail if password is not typed',
        pass: '',
        error: 'Invalid password'
      }].forEach(config => {
        it(config.test, function() {
          return signInView.failureLogin(config.pass)
          .then(alertMessage => { assert.equal(alertMessage, config.error); });
        });
      });

      it('should accept matching, long-enough passwords', function() {
        return signInView.successLogin();
      });
    });
  });
});
});
