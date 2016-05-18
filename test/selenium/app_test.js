'use strict';

const SuiteBuilder = require('./lib/make_suite');
const HOST_URL = 'http://fxbox.github.io/app';

var suiteBuilder = new SuiteBuilder('Github.io webapp', HOST_URL);

suiteBuilder.build((app) => {

  describe('open the web app', () => {

    var webAppMainPage;

    beforeEach(() => {
      webAppMainPage = app.getAppMainView();
    });

    it('should log in from web app', () => {
      return webAppMainPage.connectToFoxBox().then((setUpView) => {
        setUpView.successSignUpFromApp();
      });
    });
  });
});
