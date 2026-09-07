Feature: WebSocket message endpoint

  Scenario: Exchange a simple text message
    Given the server is running
    When I connect to the WebSocket endpoint
    And I send the WebSocket message "hello"
    Then the WebSocket response should be "WebSocket message"