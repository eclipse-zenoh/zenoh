pipeline {
  agent { label 'UbuntuVM' }
  options { skipDefaultCheckout() }
  parameters {
    gitParameter(name: 'GIT_TAG',
                 type: 'PT_BRANCH_TAG',
                 description: 'The Git tag to checkout. If not specified "master" will be checkout.',
                 defaultValue: 'master')
    string(name: 'DOCKER_TAG',
           description: 'An extra Docker tag (e.g. "latest"). By default GIT_TAG will also be used as Docker tag',
           defaultValue: '')
    booleanParam(name: 'PUBLISH_RESULTS',
                 description: 'Publish the resulting artifacts (to download site, Crates.io, DockerHub...',
                 defaultValue: false)
  }
  environment {
      LABEL = get_label()
      MACOSX_DEPLOYMENT_TARGET=10.7
  }

  stages {
    stage('[MacMini] Checkout Git TAG') {
      agent { label 'MacMini' }
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.GIT_TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/eclipse-zenoh/zenoh.git']]
                ])
      }
    }
    stage('[MacMini] Update Rust env') {
      agent { label 'MacMini' }
      steps {
        sh '''
        env
        echo "Building eclipse-zenoh-${LABEL}"
        rustup update
        '''
      }
    }

    stage('[MacMini] Build and tests') {
      agent { label 'MacMini' }
      steps {
        sh '''
        cargo build --release --all-targets
        cargo test --release
        '''
      }
    }

    stage('[MacMini] MacOS Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-macosx${MACOSX_DEPLOYMENT_TARGET}-x86-64.tgz --strip-components 2 target/release/zenohd target/release/*.dylib
        tar -czvf eclipse-zenoh-${LABEL}-examples-macosx${MACOSX_DEPLOYMENT_TARGET}-x86-64.tgz --exclude 'target/release/examples/*.*' --strip-components 3 target/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-macosx*-x86-64.tgz', name: 'zenohMacOS'
      }
    }

    stage('[MacMini] Docker build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        RUSTFLAGS='-C target-feature=-crt-static' cargo build --release --target=x86_64-unknown-linux-musl
        if [ -n "${DOCKER_TAG}" ]; then
          export EXTRA_TAG="-t eclipse/zenoh:${DOCKER_TAG}"
        fi
        docker build -t eclipse/zenoh:${LABEL} ${EXTRA_TAG} .
        '''
      }
    }

    stage('[MacMini] x86_64-unknown-linux-gnu build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/manylinux2010-x64-rust-nightly \
            /bin/bash -c "\
            cargo build --release --bins --lib --examples --target=x86_64-unknown-linux-gnu && \
            cargo deb --target=x86_64-unknown-linux-gnu -p zenoh-router && \
            cargo deb --target=x86_64-unknown-linux-gnu -p zplugin-http && \
            cargo deb --target=x86_64-unknown-linux-gnu -p zplugin_storages \
            "
        '''
      }
    }
    stage('[MacMini] x86_64-unknown-linux-gnu Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-x86_64-unknown-linux-gnu.tgz --strip-components 3 target/x86_64-unknown-linux-gnu/release/zenohd target/x86_64-unknown-linux-gnu/release/*.so
        tar -czvf eclipse-zenoh-${LABEL}-examples-x86_64-unknown-linux-gnu.tgz --exclude 'target/x86_64-unknown-linux-gnu/release/examples/*.*' --exclude 'target/x86_64-unknown-linux-gnu/release/examples/*-*' --strip-components 4 target/x86_64-unknown-linux-gnu/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-x86_64-unknown-linux-gnu.tgz, target/x86_64-unknown-linux-gnu/debian/*.deb', name: 'zenohLinux-x64'
      }
    }

    stage('[MacMini] i686-unknown-linux-gnu build') {
      agent { label 'MacMini' }
      steps {
        sh '''
        docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/manylinux2010-i686-rust-nightly \
            /bin/bash -c "\
            cargo build --release --bins --lib --examples --target=i686-unknown-linux-gnu && \
            cargo deb --target=i686-unknown-linux-gnu -p zenoh-router && \
            cargo deb --target=i686-unknown-linux-gnu -p zplugin-http && \
            cargo deb --target=i686-unknown-linux-gnu -p zplugin_storages \
            "
        '''
      }
    }
    stage('[MacMini] i686-unknown-linux-gnu Package') {
      agent { label 'MacMini' }
      steps {
        sh '''
        tar -czvf eclipse-zenoh-${LABEL}-i686-unknown-linux-gnu.tgz --strip-components 3 target/i686-unknown-linux-gnu/release/zenohd target/i686-unknown-linux-gnu/release/*.so
        tar -czvf eclipse-zenoh-${LABEL}-examples-i686-unknown-linux-gnu.tgz --exclude 'target/i686-unknown-linux-gnu/release/examples/*.*' --exclude 'target/i686-unknown-linux-gnu/release/examples/*-*' --strip-components 4 target/x86_64-unknown-linux-gnu/release/examples/*
        '''
        stash includes: 'eclipse-zenoh-*-i686-unknown-linux-gnu.tgz, target/i686-unknown-linux-gnu/debian/*.deb', name: 'zenohLinux-i686'
      }
    }

    stage('Publish to download.eclipse.org') {
      steps {
        // Unstash MacOS package to be deployed
        unstash 'zenohMacOS'
        unstash 'zenohLinux-x64'
        unstash 'zenohLinux-i686'
        sshagent ( ['projects-storage.eclipse.org-bot-ssh']) {
          sh '''
          if [ "${PUBLISH_RESULTS}" = "true" ]; then
            ssh genie.zenoh@projects-storage.eclipse.org mkdir -p /home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}
            ssh genie.zenoh@projects-storage.eclipse.org ls -al /home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}
            scp eclipse-zenoh-${LABEL}-*.tgz target/x86_64-unknown-linux-gnu/debian/*.deb target/i686-unknown-linux-gnu/debian/*.deb genie.zenoh@projects-storage.eclipse.org:/home/data/httpd/download.eclipse.org/zenoh/zenoh/${LABEL}/
          else
            echo "Publication to download.eclipse.org skipped"
          fi
          '''
        }
      }
    }

    stage('[MacMini] Publish to crates.io') {
      agent { label 'MacMini' }
      steps {
        sh '''
        if [ "${PUBLISH_RESULTS}" = "true" ]; then
          cd zenoh-util && cargo publish && cd -
          cd zenoh-protocol && cargo publish && cd -
          cd zenoh-router && cargo publish && cd -
          cd zenoh && cargo publish && cd -
        else
          echo "Publication to crates.io skipped"
        fi
        '''
      }
    }

    stage('[MacMini] Publish to Docker Hub') {
      agent { label 'MacMini' }
      steps {
        withCredentials([usernamePassword(credentialsId: 'dockerhub-bot',
            passwordVariable: 'DOCKER_HUB_CREDS_PSW', usernameVariable: 'DOCKER_HUB_CREDS_USR')])
        {
          sh '''
          if [ "${PUBLISH_RESULTS}" = "true" ]; then
            docker login -u ${DOCKER_HUB_CREDS_USR} -p ${DOCKER_HUB_CREDS_PSW}
            docker push eclipse/zenoh:${LABEL}
            if [ -n "${DOCKER_TAG}" ]; then
              docker push eclipse/zenoh:${DOCKER_TAG}
            fi
            docker logout
          else
            echo "Publication to Docker Hub skipped"
          fi
          '''
        }
      }
    }
  }
}

def get_label() {
    return env.GIT_TAG.startsWith('origin/') ? env.GIT_TAG.minus('origin/') : env.GIT_TAG
}
