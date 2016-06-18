'use strict';

const chakram = require('chakram'), expect = chakram.expect;

var Prepper = require('../lib/make_suite.js');

Prepper.makeSuite('Test Push Service locally',function(){
  var pushURI = Prepper.webPush_server.getEndpointURI();
  var pushkey = Prepper.webPush_server.getPublicKey();

  var baseSubscriptionPayload = {
     'select': {
       'id':'channel:subscription.webpush@link.mozilla.org',
       'feature': 'webpush/subscribe',
     },
     'value': {
       'subscriptions':[{
         'push_uri': Prepper.webPush_server.getEndpointURI(),
         'public_key': Prepper.webPush_server.getPublicKey()
       }]
    }
  };

  var newWebPushSubscriptionPayload =
  Object.assign({}, baseSubscriptionPayload, {
    value: {
        subscriptions: [{
          'push_uri': Prepper.webPush_server.getEndpointURI(),
          'public_key': Prepper.webPush_server.getPublicKey(),
          'auth': Prepper.webPush_server.getUserAuth()
        }]
    }
  });

  var testParams = [{
    suiteName: 'Old WebPush',
    payload: baseSubscriptionPayload
  },
  {
    suiteName: 'New WebPush',
    payload: newWebPushSubscriptionPayload
  }];

  before(Prepper.turnOnAllSimulators);
  before(Prepper.turnOnFoxbox);
  before(Prepper.foxboxManager.foxboxLogin);

  testParams.forEach(testParam => {
    describe(testParam.suiteName, function() {

      it('Check webpush service is registered',function(){  
        var pushService = false;

        return chakram.get(Prepper.foxboxManager.serviceListURL)
        .then(function(listResponse) {
          expect(listResponse).to.have.status(200);
          for (var entry of listResponse.body) {
            if (entry.adapter === 'webpush@link.mozilla.org') {
              pushService = true;
            }
          }
          expect(pushService).equals(true);
        });
      });

      it('Add push subscription',function(){
        var setter = 'channel:subscription.webpush@link.mozilla.org';
        var getter =  'channel:subscription.webpush@link.mozilla.org';
        // differs by the type of webpush std
        var setterPayload = testParam.payload;
        var getterPayload = {'id': getter, 'feature': 'webpush/subscribe'};

        return chakram.put(Prepper.foxboxManager.setterURL,setterPayload)
        .then(function(cmdResp){
         expect(cmdResp).to.have.status(200);
         // When there is no error, the payload returns null with the setter
         expect(cmdResp.body[setter]).equals(null);
         return chakram.put(Prepper.foxboxManager.getterURL,getterPayload);
        })
        .then(function(cmdResp){
          expect(cmdResp).to.have.status(200);
          expect(cmdResp.body[getter].subscriptions[0].public_key)
          .equals(Prepper.webPush_server.getPublicKey());
          expect(cmdResp.body[getter].subscriptions[0].push_uri)
          .equals(Prepper.webPush_server.getEndpointURI());
        });
      });

      it('Set resources to receive notification',function(){
        var getter = 'channel:resource.webpush@link.mozilla.org';
        var setter = 'channel:resource.webpush@link.mozilla.org';
        var setterPayload = {
          'select': {
            'id': setter,
            'feature': 'webpush/resource'
          },
            'value': {
                'resources':['livingroom', 'washroom']}};
        var getterPayload = {'id': getter, 'feature': 'webpush/resource'};

        return chakram.put(Prepper.foxboxManager.setterURL,setterPayload)
        .then(function(cmdResp){
         expect(cmdResp).to.have.status(200);
         expect(cmdResp.body[setter]).equals(null);
         return chakram.put(Prepper.foxboxManager.getterURL,getterPayload);
        })
        .then(function(cmdResp){
          expect(cmdResp).to.have.status(200);
          expect(cmdResp.body[getter]
            .resources[0]).equals('livingroom');
          expect(cmdResp.body[getter]
            .resources[1]).equals('washroom');
        });
      });

      it('Accepts notification trigger requests',function(){
        var setter = 'channel:notify.webpush@link.mozilla.org';
        var resource = 'washroom';
        var notificationText = 'lights on!';
        var payload = {
          'select': {
            'id': setter,
            'feature': 'webpush/notify-msg'
          },
          'value': {
              'resource':resource,'message':notificationText}} ;
        return chakram.put(Prepper.foxboxManager.setterURL, payload)
        .then(function(cmdResponse) {
          expect(cmdResponse).to.have.status(200);
          expect(cmdResponse.body[setter]).equals(null);
        });
      });

      describe ('Once notification is sent', function () {
        var resource = 'livingroom';
        var notificationText = 'can you encrypt this one too?';

        before(function(done) {
          var setter = 'channel:notify.webpush@link.mozilla.org';
          var payload = {
            'select': {
              'id': setter,
              'feature': 'webpush/notify-msg'
            },
            'value': {
                'resource':resource,'message':notificationText}} ;
          return chakram.put(Prepper.foxboxManager.setterURL, payload)
          .then(function(cmdResponse) {
          // collect the response from the webpush simulator 
          // after 100ms of (network)latency
          setTimeout(done,100);
          });
        });

        it('Check notification is received by Push server',function(){
          var output = JSON.parse(Prepper.webPush_server.getDecodedPushMsg());

          expect(output.message).equals(notificationText);
          expect(output.resource).equals(resource);
        });

        it('Remove subscription', function(){
          var setter = 'channel:unsubscribe.webpush@link.mozilla.org';
          var getter =  'channel:subscription.webpush@link.mozilla.org';
          var setterPayload = {
            'select': {
              'id':setter,
              'feature': 'webpush/unsubscribe'
            },
            'value': {
                'subscriptions':[{
                  'push_uri':pushURI,
                  'public_key':pushkey
                }]}};
          var getterPayload = {'id': getter, 'feature': 'webpush/subscribe'};

          return chakram.put(Prepper.foxboxManager.setterURL,setterPayload)
          .then(function(cmdResp){
           expect(cmdResp).to.have.status(200);
           expect(cmdResp.body[setter]).equals(null);
           return chakram.put(Prepper.foxboxManager.getterURL,getterPayload);
          })
          .then(function(cmdResp){
            expect(cmdResp).to.have.status(200);
            expect(cmdResp.body[getter].subscriptions.length).equals(0);
          });
        });
      });
    });
  });
});