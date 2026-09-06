Feature: SOAP acknowledgement endpoint

  Scenario: Server receives a simple SOAP request
    Given the server is running
    When I send a SOAP request to "/soap"
    Then the SOAP response content type should be "text/xml"
    And the SOAP response body should contain "SOAP message"