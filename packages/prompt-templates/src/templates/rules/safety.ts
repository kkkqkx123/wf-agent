/**
 * Security Rule Templates
 * Used to define security-related rules
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Security Rule Templates
 */
export const SAFETY_RULE_TEMPLATE: PromptTemplate = {
  id: "rules.safety",
  name: "Safety Rules",
  description: "Security-related rules",
  category: "rules",
  content: `## Security Rules

### Data Security
1. **Input Validation**: Verify and clean all user input.
2. **Sensitive Information**: Do not log or disclose sensitive information (such as passwords and keys).
3. **Data Encryption**: Encrypt sensitive data during storage and transmission.
4. **Access Control**: Implement appropriate access control and permission management.

### Code Security
1. **SQL Injection**: Use parameterized queries to prevent SQL injection attacks.
2. **XSS Protection**: Escape output to prevent XSS attacks.
3. **CSRF Protection**: Implement a CSRF token mechanism.
4. **Dependency Security**: Regularly update dependency packages to fix known vulnerabilities.

### Error Handling
1. **Error Messages**: Do not expose sensitive system information to users.
2. **Logging**: Record security-related events and errors.
3. **Exception Handling**: Properly handle exceptions to prevent information leakage.
4. **Error Recovery**: Provide a secure error recovery mechanism.

### Authentication and Authorization
1. **Strong Passwords**: Require the use of strong passwords and encourage regular password changes.
2. **Multi-Factor Authentication**: Implement multi-factor authentication.
3. **Session Management**: Manage user sessions securely.
4. **Principle of Least Privilege**: Follow the principle of least privilege.

### Other Security Measures
1. **HTTPS**: Use HTTPS for secure communication.
2. **Security Headers**: Set appropriate security response headers.
3. **Rate Limiting**: Implement API rate limiting.
4. **Security Audits**: Conduct regular security audits and tests. `,
  variables: [],
};
