Feature: FIX heartbeat acceptor

  Scenario: Respond to a FIX heartbeat
    Given the server is running
    When I send a FIX heartbeat message
    Then the FIX response should contain message type "0"
