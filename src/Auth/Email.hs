module Auth.Email
  ( sendVerificationEmail
  ) where

import           Data.Text          (Text)
import qualified Data.Text          as T
import qualified Data.Text.Lazy     as TL
import           Network.Mail.Mime  (Address (..), simpleMail')
import           Network.Mail.SMTP  (sendMailWithLoginTLS')

import           Auth.Config        (Config (..), SmtpConfig (..))

sendVerificationEmail :: Config -> Text -> Text -> IO ()
sendVerificationEmail cfg toEmail link =
  case cfgSmtp cfg of
    Nothing ->
      putStrLn $ "[dev-email] verification link for " <> T.unpack toEmail
                 <> ":\n            " <> T.unpack link
    Just smtp -> do
      let from    = Address (Just (cfgRpName cfg)) (smtpFrom smtp)
          to      = Address Nothing toEmail
          subject = "Verify your email address"
          body    = TL.fromStrict $
                      "Welcome to " <> cfgRpName cfg <> "!\n\n"
                      <> "Please verify your email address by opening:\n\n"
                      <> link <> "\n\n"
                      <> "If you did not create an account you can ignore this message.\n"
          mail    = simpleMail' to from subject body
      sendMailWithLoginTLS'
        (smtpHost smtp)
        (fromIntegral (smtpPort smtp))
        (smtpUser smtp)
        (smtpPass smtp)
        mail
