# Policies

The public policies for the OpenWorld apps. They follow [legal-review.md](../legal-review.md) §3, §4.5, §8 and §13, and they describe only what the code does ([data-flow.md](../data-flow.md)).

| Policy | Required by | Where it must be linked |
| --- | --- | --- |
| [Terms of Use](terms.md) | legal-review §4.5; store binaries need their own terms, since Apache-2.0 covers only the source | First-run screen, About, every store listing, GitHub releases |
| [Privacy Policy](privacy.md) | Apple 5.1.1(i), Google Play, CCPA | In the app, App Store Connect, Play Console, every store listing |
| [Biometric Data Policy](biometric.md) | BIPA §15(a), Texas CUBI, Colorado HB24-1130, if they apply | Beside the privacy policy |
| [Consumer Health Data Privacy Policy](washington-health-data.md) | Washington My Health My Data Act, if it applies | The website's home page |

These are drafts written without a licensed attorney. They do not replace one ([legal-review.md](../legal-review.md) §12). A policy that says something the app does not do is itself a deception risk (FTC Act §5), so any change to the data flow changes these files in the same commit.

## Before they are published

The owner fills in each marked blank. None may be guessed:

- **Effective date**: the release date.
- **Contact**: an email address, and a postal address for OneX Engineering.
- **Governing law**: the state whose law governs the Terms, and whether disputes go to court or to arbitration (legal-review §12, question 1).

A licensed attorney reviews all four files. Then each file is hosted at a stable URL and linked from the places in the table.
