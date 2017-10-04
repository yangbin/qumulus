# Contributing to Qumulus
Thanks for taking the first step to contributing to Qumulus! The following are a guidelines on how you can get started, they are not hard rules, please practice your best judgement when making changes to this document or to other files within this project.

## How can I contribute?

### Raising Issues
When you would like to raise an issue in the Qumulus project, these are some guidelines that you can follow to make it easier for maintainers to understand and identify similar reports and link pull requests and potential fixes.

```
[Short description of the Issue]

# Reproduction Steps
1. Step 1
2. Step 2
3. Step 3

# Current Behaviour

# Expected Behaviour

# Any other resources
- Include any other information here that can aid the resolution of this issue
- Screenshots or Screencasts
- Logs
```

### Pull Request
Congratulations on making a pull request to the Qumulus project! You're well on your way to shipping some code, but before that, please ensure that your code compiles with no errors/warnings and take some time to read through our [Style Guidelines](#style-guidelines). Make sure your [commit messages](#commit-messages) are clean, squash all related commits together so that they are atomic, and [code style](#code-style) is consistent.

Once you've done that, take a look at our pull request format and try and cover all the bases.
```
[Short summary of the changes made]

If required, you may include a body of text that can further provide context to the changes involved. You may also choose to attach screenshots or screencasts that can describe the fix.

# Fixes: #12, #15

# Any other resources
- Related tickets
- Related reading resources
```

## Style Guidelines

### Commit Message
In order to maintain a Git commit history that is consistent as well as concise, we prescribe to the Seven Rules of formating your Git commit message. When making commits, please keep in mind the following conventions.

#### The Seven Rules
1. Separate subject from body with a blank line
2. Limit the subject line to 50 characters
3. Capitalize the subject line
4. Do not end the subject line with a period
5. Use the imperative mood in the subject line
6. Wrap the body at 72 characters
7. Use the body to explain what and why vs. how

Read more on why Git commit messages matter [here](http://chris.beams.io/posts/git-commit/)

### Code Style
Working on the Qumulus project, we would like the experience to be as consistent for a seasoned contributor and fresh ones alike. Another way to keep things consistent is in the code style that we adopt when we are committing to the code base. If you're editing some code, please take a moment to understand how the previous contributor has styled his code. Pick up on the conventions and we will well be on our way to code style nirvana. It's ok to make mistakes, although as much as possible, we would much rather be commenting on the feature you are adding than the code style inconsistencies in your pull request.

The basis of this code style guideline is not to be restrictive but to allow for all contributors to focus on what you're trying to say rather than how you are saying it. Keeping code styles consistent is an easy way to ensure that readers won't be distracted by different conventions while reviewing code. 
